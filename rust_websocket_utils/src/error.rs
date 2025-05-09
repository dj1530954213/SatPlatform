// rust_websocket_utils/src/error.rs

//! 定义 WebSocket 工具库相关的错误类型。

use thiserror::Error; // 引入 thiserror 来简化错误类型的定义 (规则 1.2, 5.1)
// use tokio_tungstenite::tungstenite::Error as TungsteniteError; // WebSocket 底层库的错误类型 - 已在 WsError 中通过 #[from] 引入
// use serde_json; // For WsClientError and potentially WsServerError - 已在 WsError 中通过 String 承载错误信息

// 下面的 WsUtilError, WsServerError, WsClientError 是旧的错误类型，将被注释掉或删除
/*
/// `WsUtilError` 是 `rust_websocket_utils` 库中所有操作可能返回的统一错误类型。
#[derive(Error, Debug)]
pub enum WsUtilError {
    /// 当 TCP 监听器无法绑定到指定地址时发生。
    #[error("TCP 监听器绑定错误: {0}")]
    TcpBindError(#[source] std::io::Error),

    /// 当接受新的 TCP 连接失败时发生。
    #[error("接受 TCP 连接失败: {0}")]
    TcpAcceptError(#[source] std::io::Error),

    /// 当 WebSocket 握手失败时发生。
    #[error("WebSocket 握手错误: {0}")]
    HandshakeError(#[from] tokio_tungstenite::tungstenite::Error),

    /// 当消息序列化 (例如转为 JSON) 失败时发生。
    /// (虽然在此模块中不直接使用，但作为通用错误类型包含进来，以供将来使用，比如发送 WsMessage)
    #[error("消息序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// 当发送 WebSocket 消息失败时发生。
    /// 注意：TungsteniteError 包含多种情况，这里概括性地表示发送错误。
    #[error("消息发送错误: {0}")]
    MessageSendError(tokio_tungstenite::tungstenite::Error),

    /// 当接收 WebSocket 消息失败时发生。
    #[error("消息接收错误: {0}")]
    MessageReceiveError(tokio_tungstenite::tungstenite::Error),

    /// 表示 WebSocket 连接已关闭。
    /// TungsteniteError::ConnectionClosed 通常用于此情况。
    #[error("连接已关闭")]
    ConnectionClosed,

    /// 通用 I/O 错误，用于其他未明确分类的 I/O 问题。
    #[error("I/O 错误: {0}")]
    GenericIoError(String),
}

/// Errors that can occur in WebSocket server operations.
#[derive(Error, Debug)]
pub enum WsServerError {
    #[error("Server IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("WebSocket protocol error: {0}")]
    ProtocolError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error for server: {0}")]
    SerializationError(serde_json::Error), // Renamed from previous thought to avoid direct #[from] if custom logic needed
    #[error("Client with ID {0} not found")]
    ClientNotFound(String), // uuid::Uuid might be better here
    #[error("Address parsing error: {0}")]
    AddrParseError(String), // Simplified from AddrParseError if direct from is not always applicable
    #[error("Message handling error: {0}")]
    MessageHandlerError(String),
    #[error("Failed to acquire lock: {0}")]
    LockError(String),
    #[error("Generic server error: {0}")]
    GenericError(String),
}

/// Errors that can occur in WebSocket client operations.
#[derive(Error, Debug)]
pub enum WsClientError {
    #[error("URL parsing error: {0}")]
    UrlParseError(String),
    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Failed to send message: {0}")]
    SendError(String),
    #[error("Failed to receive message: {0}")]
    ReceiveError(String),
    #[error("Serialization error for client: {0}")]
    SerializationError(serde_json::Error), // Keeping this specific to client
    #[error("Deserialization error for client: {0}")]
    DeserializationError(serde_json::Error), // Keeping this specific to client
    #[error("Generic client error: {0}")]
    GenericError(String),
}
*/

/// WebSocket 工具库的统一错误类型。
#[derive(Error, Debug)]
pub enum WsError {
    /// 当 serde 序列化失败时返回。
    /// 包含具体的序列化错误信息。
    #[error("序列化错误: {0}")] // 中文化
    SerializationError(String),

    /// 当 serde 反序列化失败时返回。
    /// 包含具体的反序列化错误信息。
    #[error("反序列化错误: {0}")] // 中文化
    DeserializationError(String),

    /// WebSocket 协议相关的错误。
    /// 例如，连接问题、消息格式不正确等。
    #[error("WebSocket协议错误: {0}")] // 中文化
    WebSocketProtocolError(#[from] tokio_tungstenite::tungstenite::Error),

    /// 底层 I/O 错误。
    #[error("I/O错误: {0}")] // 中文化
    IoError(#[from] std::io::Error),

    /// 当尝试发送消息到一个已关闭的通道时发生。
    #[error("发送错误: 通道已关闭")] // 中文化
    SendErrorClosed,

    /// 当尝试发送消息但对方已断开连接时发生。
    #[error("发送错误: 接收端已断开连接 ({0})")] // 中文化
    SendErrorDisconnected(String),

    /// 连接超时错误。
    #[error("连接超时")] // 中文化
    ConnectionTimeout,

    /// 无效的 URL 格式。
    #[error("无效的URL: {0}")] // 中文化
    InvalidUrl(String),

    /// 未连接错误，当尝试在未建立连接时进行操作。
    #[error("未连接")] // 中文化
    NotConnected,

    /// 通用消息错误，用于其他未明确分类的错误。
    #[error("消息错误: {0}")] // 中文化
    Message(String),
}

// 移除最后的占位注释
// // 暂时为空，后续会根据开发步骤添加具体错误枚举或结构。 