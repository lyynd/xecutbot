use anyhow::Result;
use chrono::{Local, Locale, NaiveDate, TimeDelta};
use futures::FutureExt;
use itertools::Itertools;
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

use teloxide::{
    dispatching::dialogue::GetChatId as _,
    prelude::*,
    sugar::request::{RequestLinkPreviewExt, RequestReplyExt as _},
    types::{MessageId, ParseMode, ReactionType},
    utils::command::BotCommands,
};

use crate::{
    config::TelegramBotConfig,
    visits::{Visit, VisitStatus, VisitUpdate},
};
use crate::{utils::today, visits::Visits};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uid(UserId);

impl From<i64> for Uid {
    fn from(value: i64) -> Self {
        Uid(UserId(value as u64))
    }
}

impl From<Uid> for i64 {
    fn from(val: Uid) -> Self {
        val.0.0 as i64
    }
}

#[derive(BotCommands, Clone, Copy)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(
        description = "📮 Репостнуть пост в live канал (реплайни на пост, доступно только резидентам)"
    )]
    PostLive,
    #[command(description = "ℹ️ Посмотреть что сейчас происходит в хакспейсе")]
    Status,
    #[command(description = "🗓️ Посмотреть кто собирается в хакспейс в ближайшие дни")]
    GetVisits,
    #[command(
        description = "🗓️ Запланировать зайти в хакспейс (опционально дата в формате YYYY-MM-DD и описание зачем)"
    )]
    PlanVisit,
    #[command(
        description = "🤔 Передумать заходить в хакспейс (опционально дата в формате YYYY-MM-DD)"
    )]
    UnplanVisit,
    #[command(description = "👷 Отметиться как зашедший (опционально описание зачем)")]
    CheckIn,
    #[command(description = "🌆 Отметиться как ушедший")]
    CheckOut,
    #[command(description = "🔃 Сделать закреп с текущей информацией о спейсе")]
    LiveStatus,
}

fn strip_command(text: &str) -> &str {
    if text.starts_with('/') {
        text.split_once(' ').map(|p| p.1).unwrap_or("")
    } else {
        text
    }
}

fn parse_day_purpose(text: &str) -> (NaiveDate, &str) {
    if let Some(purpose) = text.strip_prefix("завтра") {
        return (today() + TimeDelta::days(1), purpose.trim());
    }
    if let Some(purpose) = text.strip_prefix("послезавтра") {
        return (today() + TimeDelta::days(2), purpose.trim());
    }

    let Ok((date, purpose)) = NaiveDate::parse_and_remainder(text, "%Y-%m-%d") else {
        return (today(), text.trim());
    };

    (date, purpose.trim())
}

fn parse_visit_text(author: UserId, msg: &str) -> VisitUpdate {
    let (day, purpose) = parse_day_purpose(msg);
    VisitUpdate {
        person: Uid(author),
        day,
        purpose: if purpose.is_empty() {
            None
        } else {
            Some(purpose.to_owned())
        },
        status: VisitStatus::Planned,
    }
}

fn format_date(date: NaiveDate) -> String {
    let format = if date - today() > TimeDelta::days(60) {
        "%d %B %Y (%A)"
    } else {
        "%d %B (%A)"
    };
    let base_date = date
        .format_localized(format, Locale::ru_RU)
        .to_string()
        .to_lowercase();
    if date - today() == TimeDelta::days(0) {
        return "сегодня, ".to_owned() + &base_date;
    }
    if date - today() == TimeDelta::days(1) {
        return "завтра, ".to_owned() + &base_date;
    }
    if date - today() == TimeDelta::days(2) {
        return "послезавтра, ".to_owned() + &base_date;
    }
    base_date
}

struct PersonDetails {
    resident: bool,
    display_name: String,
    link: String,
}

const LIVE_UPDATE_INTERVAL: Duration = Duration::from_secs(2);

pub struct TelegramBot {
    config: TelegramBotConfig,
    bot: Bot,
    visits: Visits,
    status_message_id: RwLock<Option<MessageId>>,
}

impl TelegramBot {
    pub async fn new(config: TelegramBotConfig, visits: Visits) -> Result<Arc<TelegramBot>> {
        let bot = Bot::new(config.bot_token.clone());
        let status_message_id = RwLock::new(Self::load_status_message_id(visits.pool()).await?);
        Ok(Arc::new(TelegramBot {
            config,
            bot,
            visits,
            status_message_id,
        }))
    }

    pub async fn spawn_update_live_task(self: &Arc<Self>) -> CancellationToken {
        let cancellation_token = CancellationToken::new();
        let result = cancellation_token.clone();
        let self_clone = self.clone();

        tokio::task::spawn(async move {
            let mut interval = tokio::time::interval(LIVE_UPDATE_INTERVAL);

            let mut last_live_status = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    _ = cancellation_token.cancelled() => { break }
                };
                log::trace!("Updating status message");
                let new_live_status = match self_clone.get_status().await {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Error getting live status: {}", e);
                        continue;
                    }
                };
                if last_live_status.is_none_or(|ref v| v != &new_live_status)
                    && let Err(e) = self_clone
                        .update_live_status_message(&new_live_status)
                        .await
                {
                    log::info!("Error updating status message: {}", e);
                }
                last_live_status = Some(new_live_status);
            }
        });

        result
    }

    pub async fn run(self: Arc<Self>) {
        log::info!("Starting Telegram bot");

        if let Err(e) = self.bot.set_my_commands(Command::bot_commands()).await {
            log::error!("Can't set commands: {e}");
        }

        let self_clone = self.clone();

        let handler = move |msg: Message, cmd: Command| {
            let self_clone = self_clone.clone();
            async move {
                let res = AssertUnwindSafe(self_clone.clone().handle_message(&msg, cmd))
                    .catch_unwind()
                    .await;
                if matches!(res, Err(_) | Ok(Err(_))) {
                    self_clone
                        .bot
                        .send_message(msg.chat.id, "💥 Что-то пошло не так, найдите админа")
                        .reply_to(msg.id)
                        .await?;
                    if let Ok(e) = res {
                        return e;
                    }
                }
                Ok(())
            }
        };

        Dispatcher::builder(
            self.bot.clone(),
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handler),
        )
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    }

    async fn handle_message(self: Arc<Self>, msg: &Message, cmd: Command) -> Result<()> {
        match cmd {
            Command::PostLive => self.handle_post_live(msg).await,
            Command::Status => self.handle_status(msg).await,
            Command::GetVisits => self.handle_get_visits(msg).await,
            Command::PlanVisit => self.handle_plan_visit(msg).await,
            Command::UnplanVisit => self.handle_unplan_visit(msg).await,
            Command::CheckIn => self.handle_check_in(msg).await,
            Command::CheckOut => self.handle_check_out(msg).await,
            Command::LiveStatus => self.handle_live_status(msg).await,
        }
    }

    async fn is_resident(&self, id: UserId) -> Result<bool> {
        Ok(self
            .bot
            .get_chat_member(self.config.private_chat_id, id)
            .await?
            .is_present())
    }

    async fn fetch_persons_details(
        &self,
        persons: impl IntoIterator<Item = Uid>,
    ) -> Result<HashMap<Uid, PersonDetails>> {
        Ok(
            futures::future::try_join_all(persons.into_iter().unique().map(
                async |user| -> Result<_> {
                    let user_id = user.0;
                    let chat_member = self
                        .bot
                        .get_chat_member(self.config.public_chat_id, user_id)
                        .await?;
                    let resident = self.is_resident(user_id).await?;
                    let display_name = if let Some(ref username) = chat_member.user.username {
                        username.clone()
                    } else {
                        chat_member.user.full_name()
                    };
                    let link = chat_member.user.preferably_tme_url().to_string();
                    Ok((
                        user,
                        PersonDetails {
                            resident,
                            display_name,
                            link,
                        },
                    ))
                },
            ))
            .await?
            .into_iter()
            .collect::<HashMap<_, _>>(),
        )
    }

    async fn handle_post_live(&self, msg: &Message) -> Result<()> {
        let Some(chat_id) = msg.chat_id() else {
            log::debug!("Message does not have a chat");
            return Ok(());
        };

        if chat_id != self.config.public_chat_id {
            log::debug!("Message not in the public chat");
            return Ok(());
        }

        if !self
            .is_resident(msg.from.as_ref().expect("message to have from").id)
            .await?
        {
            log::debug!("User is not a private chat member");
            self.bot
                .send_message(chat_id, "❌ Нужно быть резидентом")
                .reply_to(msg.id)
                .disable_notification(true)
                .await?;
            return Ok(());
        }

        let Some(original_message) = msg.reply_to_message() else {
            log::debug!("Message is not a reply");
            self.bot
                .send_message(chat_id, "❌ Надо ответить на сообщение")
                .reply_to(msg.id)
                .disable_notification(true)
                .await?;
            return Ok(());
        };

        self.bot
            .send_message(
                self.config.public_channel_id,
                original_message
                    .url()
                    .expect("original message to have URL"),
            )
            .disable_link_preview(true)
            .await?;

        log::debug!("Message posted");

        let forwarded_message_url = self
            .bot
            .forward_message(self.config.public_channel_id, chat_id, original_message.id)
            .await?
            .url()
            .expect("forwarded message to have URL");

        log::debug!("Original message forwarded");

        let channel_name = self
            .bot
            .get_chat(self.config.public_channel_id)
            .await?
            .title()
            .unwrap_or("канал")
            .to_owned();

        self.bot
            .send_message(
                chat_id,
                format!("✅ Запостил в <a href=\"{forwarded_message_url}\">{channel_name}</a>"),
            )
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .disable_notification(true)
            .await?;

        Ok(())
    }

    fn format_person_link(&self, details: &PersonDetails) -> String {
        format!(
            "<a href=\"{}\">{}</a>{}",
            details.link,
            details.display_name,
            if details.resident { "®️" } else { "" }
        )
    }

    fn format_visit_without_status(&self, v: &Visit, details: &PersonDetails) -> String {
        format!(
            "{}{}",
            self.format_person_link(details),
            if !v.purpose.is_empty() {
                format!(": \"{}\"", v.purpose)
            } else {
                "".to_owned()
            }
        )
    }

    async fn get_status(&self) -> Result<String> {
        let today = today();
        let mut visits = self.visits.get_visits(today, today).await?;

        let details = self
            .fetch_persons_details(visits.iter().map(|v| v.person))
            .await?;

        visits.sort_by_key(|v| if details[&v.person].resident { 0 } else { 1 });

        let mut status = String::new();

        let checked_in = visits
            .iter()
            .filter(|v| v.status == VisitStatus::CheckedIn)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        if !checked_in.is_empty() {
            status.push_str("🟢 В хакспейсе сейчас кто-то есть, так что можно зайти.\n\n");
            status.push_str("👷 Сейчас в хакспейсе:\n");
            status.push_str(&checked_in);
        } else {
            status.push_str("🔴 В хакспейсе сейчас никого нет, можешь попробовать спросить, может кто-то из резидентов захочет прийти.");
        }

        let planned = visits
            .iter()
            .filter(|v| v.status == VisitStatus::Planned)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        if !planned.is_empty() {
            status.push_str("\n\n🗓️ Планировали зайти:\n");
            status.push_str(&planned);
        }

        let left = visits
            .iter()
            .filter(|v| v.status == VisitStatus::CheckedOut)
            .map(|v| self.format_visit_without_status(v, &details[&v.person]))
            .join("\n");

        if !left.is_empty() {
            status.push_str("\n\n🌆 Уже ушли:\n");
            status.push_str(&left);
        }

        Ok(status)
    }

    async fn handle_status(&self, msg: &Message) -> Result<()> {
        if msg
            .chat_id()
            .is_some_and(|c| c == self.config.public_chat_id)
            && let Some(msg_id) = self.get_status_message_id()
        {
            self.bot
                .send_message(
                    msg.chat.id,
                    format!(
                        "Посмотри в <a href=\"{}\">закрепе</a>",
                        Message::url_of(self.config.public_chat_id, None, msg_id)
                            .expect("should be able to create url of live status message")
                    ),
                )
                .parse_mode(ParseMode::Html)
                .disable_link_preview(true)
                .disable_notification(true)
                .await?;
            return Ok(());
        }

        let status = self.get_status().await?;

        self.bot
            .send_message(msg.chat.id, status)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .disable_notification(true)
            .await?;

        Ok(())
    }

    fn format_visit(&self, v: &Visit, details: &PersonDetails) -> String {
        let status_str = match v.status {
            VisitStatus::Planned => "",
            VisitStatus::CheckedIn => " (сейчас в спейсе 👷)",
            VisitStatus::CheckedOut => " (ушёл 🌆)",
        };
        format!(
            "{}{}",
            self.format_visit_without_status(v, details),
            status_str
        )
    }

    fn format_day<'a>(
        &self,
        vs: impl IntoIterator<Item = &'a Visit>,
        details: &HashMap<Uid, PersonDetails>,
    ) -> String {
        vs.into_iter()
            .sorted_by_key(|v| if details[&v.person].resident { 0 } else { 1 })
            .map(|v| self.format_visit(v, &details[&v.person]))
            .join("\n")
    }

    fn format_visits(&self, mut vs: Vec<Visit>, details: &HashMap<Uid, PersonDetails>) -> String {
        vs.sort_by_key(|v| v.day);
        let plans = vs
            .chunk_by(|v1, v2| v1.day == v2.day)
            .map(|vs| {
                let day = vs[0].day;
                format!("{}:\n{}", format_date(day), self.format_day(vs, details))
            })
            .join("\n\n");
        let mut result = String::new();
        if !plans.is_empty() {
            result.push_str("🗓️ Планы посещений на ближайшие полгода:\n\n");
            result.push_str(&plans);
        } else {
            result.push_str("😔 Нет никаких планов");
        }
        result
    }

    async fn handle_get_visits(&self, msg: &Message) -> Result<()> {
        let visits = self
            .visits
            .get_visits(today(), today() + TimeDelta::days(185)) // If you change this, also change text in `format_visits`
            .await?;

        let details = self
            .fetch_persons_details(visits.iter().map(|v| v.person))
            .await?;

        let formated_visits = self.format_visits(visits, &details);

        self.bot
            .send_message(msg.chat.id, formated_visits)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .disable_notification(true)
            .await?;

        Ok(())
    }

    async fn load_status_message_id(pool: &SqlitePool) -> Result<Option<MessageId>> {
        let message_id = sqlx::query!("SELECT message_id FROM status_messages")
            .map(|r| r.message_id)
            .fetch_optional(pool)
            .await?
            .map(|id| MessageId(id as i32));
        Ok(message_id)
    }

    async fn save_status_message_id(pool: &SqlitePool, id: Option<MessageId>) -> Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query!("DELETE FROM status_messages")
            .execute(&mut *tx)
            .await?;
        if let Some(id) = id {
            sqlx::query!("INSERT INTO status_messages (message_id) VALUES (?1)", id.0)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn set_status_message_id(&self, id: Option<MessageId>) -> Result<()> {
        Self::save_status_message_id(self.visits.pool(), id).await?;
        *self.status_message_id.write().unwrap() = id;
        Ok(())
    }

    fn get_status_message_id(&self) -> Option<MessageId> {
        *self.status_message_id.read().unwrap()
    }

    async fn handle_live_status(&self, msg: &Message) -> Result<()> {
        let Some(chat_id) = msg.chat_id() else {
            log::debug!("Message does not have a chat");
            return Ok(());
        };

        if chat_id != self.config.public_chat_id {
            log::debug!("Message not in the public chat");
            return Ok(());
        }

        if !self
            .is_resident(msg.from.as_ref().expect("message to have from").id)
            .await?
        {
            log::debug!("User is not a private chat member");
            self.bot
                .send_message(chat_id, "❌ Нужно быть резидентом")
                .reply_to(msg.id)
                .disable_notification(true)
                .await?;
            return Ok(());
        }

        if let Some(msg_id) = self.get_status_message_id() {
            self.bot
                .delete_message(msg.chat_id().unwrap(), msg_id)
                .await?;
            self.set_status_message_id(None).await?;
        }

        let msg_id = self
            .bot
            .send_message(
                msg.chat_id().unwrap(),
                Self::get_full_live_status(&self.get_status().await?),
            )
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .disable_notification(true)
            .await?
            .id;
        self.set_status_message_id(Some(msg_id)).await?;

        self.bot.pin_chat_message(chat_id, msg_id).await?;

        Ok(())
    }

    fn get_full_live_status(live_status: &str) -> String {
        live_status.to_owned()
            + "\n\nОбновлено: "
            + &Local::now()
                .format_localized("%c", Locale::ru_RU)
                .to_string()
    }

    async fn update_live_status_message(&self, live_status: &str) -> Result<()> {
        let Some(msg_id) = self.get_status_message_id() else {
            return Ok(());
        };

        self.bot
            .edit_message_text(
                self.config.public_chat_id,
                msg_id,
                Self::get_full_live_status(live_status),
            )
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await?;
        Ok(())
    }

    async fn handle_plan_visit(&self, msg: &Message) -> Result<()> {
        let msg_text = strip_command(msg.text().expect("message to have text"));

        let visit_update = parse_visit_text(
            msg.from.as_ref().expect("message to have author").id,
            msg_text,
        );

        let new = self.visits.upsert_visit(visit_update.clone()).await?;

        self.bot
            .send_message(
                msg.chat.id,
                format!(
                    "✅🗓️ {} план зайти в хакспейс {}{}",
                    if new {
                        "Добавил"
                    } else {
                        "Обновил"
                    },
                    format_date(visit_update.day),
                    if let Some(p) = visit_update.purpose {
                        format!(": \"{p}\"")
                    } else {
                        "".to_owned()
                    }
                ),
            )
            .reply_to(msg.id)
            .disable_notification(true)
            .await?;

        if msg_text == "panic" {
            panic!("ayaya");
        }

        if msg_text == "error" {
            return Err(anyhow::anyhow!("ayayaya"));
        }

        Ok(())
    }

    async fn handle_unplan_visit(&self, msg: &Message) -> Result<()> {
        let msg_text = strip_command(msg.text().expect("message to have text"));

        let visit = parse_visit_text(
            msg.from.as_ref().expect("message to have author").id,
            msg_text,
        );

        self.visits.delete_visit(visit.person, visit.day).await?;

        self.bot
            .set_message_reaction(msg.chat.id, msg.id)
            .reaction(vec![ReactionType::Emoji {
                emoji: "✍".to_owned(),
            }])
            .await?;

        Ok(())
    }

    async fn handle_check_in(&self, msg: &Message) -> Result<()> {
        let person = Uid(msg.from.as_ref().expect("message to have author").id);
        let day = today();
        let purpose_raw = strip_command(msg.text().expect("message to have text"));
        let purpose = if purpose_raw.is_empty() {
            None
        } else {
            Some(purpose_raw.to_owned())
        };
        let visit_update = VisitUpdate {
            person,
            day,
            purpose: purpose.clone(),
            status: VisitStatus::CheckedIn,
        };
        self.visits.upsert_visit(visit_update).await?;
        self.bot
            .set_message_reaction(msg.chat.id, msg.id)
            .reaction(vec![ReactionType::Emoji {
                emoji: "✍".to_owned(),
            }])
            .await?;
        Ok(())
    }

    async fn handle_check_out(&self, msg: &Message) -> Result<()> {
        let person = Uid(msg.from.as_ref().expect("message to have author").id);
        let day = today();
        let visit_update = VisitUpdate {
            person,
            day,
            purpose: None,
            status: VisitStatus::CheckedOut,
        };
        self.visits.upsert_visit(visit_update).await?;
        self.bot
            .set_message_reaction(msg.chat.id, msg.id)
            .reaction(vec![ReactionType::Emoji {
                emoji: "✍".to_owned(),
            }])
            .await?;
        Ok(())
    }
}
