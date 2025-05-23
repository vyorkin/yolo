use config::{Config, ConfigError};
use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;

use crate::server_env::ServerEnv;

#[derive(Deserialize)]
pub struct ServerConfig {
    pub host: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub base_url: String,
}

impl ServerConfig {
    pub fn read() -> Result<Self, ConfigError> {
        let base_path =
            std::env::current_dir().expect("failed to determine the currenct directory");

        let config_dir = base_path.join("config");
        let server_env: ServerEnv = std::env::var("SERVER_ENV")
            .unwrap_or_else(|_| ServerEnv::Local.as_str().to_string())
            .try_into()
            .expect("failed to parse SERVER_ENV");

        let base_config = config::File::from(config_dir.join("base")).required(true);
        let env_config = config::File::from(config_dir.join(server_env.as_str())).required(true);

        let config_builder = Config::builder()
            .add_source(base_config)
            .add_source(env_config)
            .add_source(config::Environment::with_prefix("server").separator("__"))
            .build()?;

        config_builder.try_deserialize()
    }
}
