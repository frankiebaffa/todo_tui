#[derive(Clone)]
pub enum LogType {
    Log,
    Warning,
    Error,
}
#[derive(Clone)]
pub struct LogMsg {
    pub log_type: LogType,
    pub message: String,
}
impl LogMsg {
    pub fn log(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Log;
        Self { log_type, message, }
    }
    pub fn warn(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Warning;
        Self { log_type, message, }
    }
    pub fn error(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Error;
        Self { log_type, message, }
    }
}
