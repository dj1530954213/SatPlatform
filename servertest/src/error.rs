use thiserror::Error;

/// 应用的主要错误类型
/// 
/// 这个枚举定义了应用中可能出现的各种错误类型。
/// 每一种错误类型都包含了相关的错误信息，以便进行调试和错误处理。
#[derive(Error, Debug)]
pub enum AppError {
    #[error("WebSocket 服务错误: {0}")]
    WebSocketService(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("数据库错误: {0}")]
    DatabaseError(String),

    #[error("未知错误: {0}")]
    Unknown(String),
} 