use std::sync::Arc;

use log::{debug, info};
use teloxide::{
    dispatching::dialogue::GetChatId as _,
    prelude::*,
    sugar::request::{RequestLinkPreviewExt, RequestReplyExt as _},
    types::ParseMode,
    utils::command::BotCommands,
};

use crate::config::Config;

#[derive(BotCommands, Clone, Copy)]
#[command(rename_rule = "lowercase")]
enum Command {
    PostLive,
}

pub(crate) struct Handler {
    config: Config,
    bot: Bot,
}

impl Handler {
    pub(crate) fn new(config: Config) -> Arc<Handler> {
        let bot = Bot::new(config.telegram_bot_token.clone());
        Arc::new(Handler { config, bot })
    }

    pub(crate) async fn run(self: Arc<Self>) {
        info!("Starting Telegram bot");

        Command::repl(self.bot.clone(), move |msg: Message, cmd: Command| {
            self.clone().handle_message(msg, cmd)
        })
        .await;
    }

    async fn handle_message(self: Arc<Self>, msg: Message, cmd: Command) -> ResponseResult<()> {
        match cmd {
            Command::PostLive => self.handle_post_live(msg).await,
        }
    }

    async fn handle_post_live(self: Arc<Self>, msg: Message) -> ResponseResult<()> {
        debug!("Got message");

        let Some(chat_id) = msg.chat_id() else {
            debug!("Message does not have a chat");
            return Ok(());
        };

        if chat_id != self.config.public_chat_id {
            debug!("Message not in the public chat");
            return Ok(());
        }

        let Some(ref from) = msg.from else {
            debug!("Message does not have a user");
            return Ok(());
        };

        debug!("Trying to get private member status");

        let private_member = self
            .bot
            .get_chat_member(self.config.private_chat_id, from.id)
            .await?;

        debug!("Got private member status {private_member:?}");

        if !private_member.is_present() {
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
                original_message.url().expect("Original message has URL"),
            )
            .disable_link_preview(true)
            .await?;

        debug!("Message posted");

        let forwarded_message_url = self
            .bot
            .forward_message(self.config.public_channel_id, chat_id, original_message.id)
            .await?
            .url()
            .expect("Forwarded message has URL");

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
}
