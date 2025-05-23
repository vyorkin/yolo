pub enum ServerEnv {
    Local,
    Production,
}

impl ServerEnv {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerEnv::Local => "local",
            ServerEnv::Production => "production",
        }
    }
}

impl TryFrom<String> for ServerEnv {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!("{} is not supported environment value", other)),
        }
    }
}
