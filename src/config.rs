use serde_derive::Deserialize;
use teloxide::types::ChatId;

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramBotConfig {
    pub bot_token: String,
    pub public_chat_id: ChatId,
    pub private_chat_id: ChatId,
    pub public_channel_id: ChatId,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DbConfig {
    pub sqlite_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub telegram_bot: TelegramBotConfig,
    pub db: DbConfig,
}

impl Config {
    pub fn new(base_name: &str) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name(base_name))
            .add_source(config::Environment::with_prefix(base_name))
            .build()?
            .try_deserialize()
    }
}
