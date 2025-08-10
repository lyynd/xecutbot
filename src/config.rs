use std::path::PathBuf;

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
    pub fn new(env_prefix: &str, config_files: Vec<PathBuf>) -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();
        for config_file in config_files {
            builder = builder.add_source(config::File::from(config_file));
        }
        builder
            .add_source(config::Environment::with_prefix(env_prefix))
            .build()?
            .try_deserialize()
    }
}
