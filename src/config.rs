use serde_derive::Deserialize;
use teloxide::types::ChatId;

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Config {
    pub telegram_bot_token: String,
    pub public_chat_id: ChatId,
    pub private_chat_id: ChatId,
    pub public_channel_id: ChatId,
}

impl Config {
    pub(crate) fn new() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name("xecut_bot"))
            .add_source(config::Environment::with_prefix("xecut_bot"))
            .build()?
            .try_deserialize()
    }
}
