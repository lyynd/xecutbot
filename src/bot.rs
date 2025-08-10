use anyhow::Result;
use chrono::NaiveDate;
use futures::FutureExt;
use itertools::Itertools;
use std::{panic::AssertUnwindSafe, sync::Arc};

use log::{debug, info};
use teloxide::{
    dispatching::dialogue::GetChatId as _,
    prelude::*,
    sugar::request::{RequestLinkPreviewExt, RequestReplyExt as _},
    types::ParseMode,
    utils::command::BotCommands,
};

use crate::{
    config::TelegramBotConfig,
    visits::{Visit, VisitStatus, VisitUpdate},
};
use crate::{utils::today, visits::Visits};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    #[command(description = "Репостнуть пост в live канал (реплайни на пост)")]
    PostLive,
    #[command(description = "Посмотреть кто собирается в хакспейс")]
    GetVisits,
    #[command(
        description = "Запланировать зайти в хакспейс (опционально дата в формате YYYY-MM-DD и описание зачем)"
    )]
    PlanVisit,
    #[command(
        description = "Передумать заходить в хакспейс (опционально дата в формате YYYY-MM-DD)"
    )]
    UnplanVisit,
    #[command(description = "Отметиться как зашедший (опционально описание зачем)")]
    CheckIn,
    #[command(description = "Отметиться как ушедший")]
    CheckOut,
}

pub struct TelegramBot {
    config: TelegramBotConfig,
    bot: Bot,
    visits: Visits,
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
        return (today().succ_opt().unwrap_or(today()), purpose.trim());
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

impl TelegramBot {
    pub async fn new(config: TelegramBotConfig, visits: Visits) -> Result<Arc<TelegramBot>> {
        let bot = Bot::new(config.bot_token.clone());
        Ok(Arc::new(TelegramBot {
            config,
            bot,
            visits,
        }))
    }

    pub async fn run(self: Arc<Self>) {
        info!("Starting Telegram bot");

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
                        .send_message(msg.chat.id, "Что-то пошло не так, найдите админа")
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
            Command::GetVisits => self.handle_get_visits(msg).await,
            Command::PlanVisit => self.handle_plan_visit(msg).await,
            Command::UnplanVisit => self.handle_unplan_visit(msg).await,
            Command::CheckIn => self.handle_check_in(msg).await,
            Command::CheckOut => self.handle_check_out(msg).await,
        }
    }

    async fn is_resident(&self, id: UserId) -> Result<bool> {
        Ok(self
            .bot
            .get_chat_member(self.config.private_chat_id, id)
            .await?
            .is_present())
    }

    async fn handle_post_live(&self, msg: &Message) -> Result<()> {
        debug!("Got message");

        let Some(chat_id) = msg.chat_id() else {
            debug!("Message does not have a chat");
            return Ok(());
        };

        if chat_id != self.config.public_chat_id {
            debug!("Message not in the public chat");
            return Ok(());
        }

        if !self
            .is_resident(msg.from.as_ref().expect("message to have from").id)
            .await?
        {
            debug!("User is not a private chat member");
            self.bot
                .send_message(chat_id, "Нужно быть резидентом")
                .reply_to(msg.id)
                .await?;
            return Ok(());
        }

        let Some(original_message) = msg.reply_to_message() else {
            debug!("Message is not a reply");
            self.bot
                .send_message(chat_id, "Надо ответить на сообщение")
                .reply_to(msg.id)
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

        debug!("Message posted");

        let forwarded_message_url = self
            .bot
            .forward_message(self.config.public_channel_id, chat_id, original_message.id)
            .await?
            .url()
            .expect("forwarded message to have URL");

        debug!("Original message forwarded");

        self.bot
            .send_message(
                chat_id,
                format!("Запостил в <a href=\"{forwarded_message_url}\">Xecut Live</a>"),
            )
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await?;

        Ok(())
    }

    async fn format_user(&self, resident: bool, user: Uid) -> Result<String> {
        let user = self
            .bot
            .get_chat_member(self.config.public_chat_id, user.0)
            .await?
            .user;

        let display_name = if let Some(ref username) = user.username {
            username.clone()
        } else {
            user.full_name()
        };

        Ok(format!(
            "<a href=\"{}\">{}</a>{}",
            user.preferably_tme_url(),
            display_name,
            if resident { "Ⓡ" } else { "" }
        ))
    }

    async fn format_visit(&self, resident: bool, v: &Visit) -> Result<String> {
        let status_str = match v.status {
            VisitStatus::Planned => "",
            VisitStatus::CheckedIn => "(сейчас в спейсе)",
            VisitStatus::CheckedOut => "(ушёл)",
        };
        Ok(format!(
            "{}{} {}",
            self.format_user(resident, v.person).await?,
            if !v.purpose.is_empty() {
                format!(" хочет {}", v.purpose)
            } else {
                "".to_owned()
            },
            status_str
        ))
    }

    async fn format_day(&self, vs: impl IntoIterator<Item = &Visit>) -> Result<String> {
        let mut data = futures::future::try_join_all(vs.into_iter().map(async |v| -> Result<_> {
            let resident = self.is_resident(v.person.0).await?;
            Ok((resident, self.format_visit(resident, v).await?))
        }))
        .await?;
        data.sort_by_key(|(resident, _)| if *resident { 0 } else { 1 });
        Ok(data.into_iter().map(|p| p.1).join("\n"))
    }

    async fn format_visits(&self, mut vs: Vec<Visit>) -> Result<String> {
        vs.sort_by_key(|v| v.day);
        let mut result = futures::future::try_join_all(vs.chunk_by(|v1, v2| v1.day == v2.day).map(
            async |vs| -> Result<_> {
                let day = vs[0].day;
                Ok(format!(
                    "Планировали зайти {} ({}):\n{}",
                    day,
                    day.format("%A"),
                    self.format_day(vs).await?
                ))
            },
        ))
        .await?
        .join("\n\n");
        if result.is_empty() {
            result = "Нет никаких планов".to_owned();
        }
        Ok(result)
    }

    async fn handle_get_visits(&self, msg: &Message) -> Result<()> {
        let visits = self.visits.get_visits(today()).await?;

        self.bot
            .send_message(msg.chat.id, self.format_visits(visits).await?)
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
                    "{} план зайти в хакспейс {}{}",
                    if new {
                        "Добавил"
                    } else {
                        "Обновил"
                    },
                    visit_update.day,
                    if let Some(p) = visit_update.purpose {
                        format!(" чтобы {p}")
                    } else {
                        "".to_owned()
                    }
                ),
            )
            .reply_to(msg.id)
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
            .send_message(
                msg.chat.id,
                format!("Удалил план зайти в хакспейс {}", visit.day),
            )
            .reply_to(msg.id)
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
            .send_message(
                msg.chat.id,
                format!(
                    "Отметил как зашедшего{}",
                    if let Some(p) = purpose {
                        format!(" чтобы: {p}")
                    } else {
                        "".to_string()
                    }
                ),
            )
            .reply_to(msg.id)
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
            .send_message(msg.chat.id, "Отметил как ушедшего")
            .reply_to(msg.id)
            .await?;
        Ok(())
    }
}
