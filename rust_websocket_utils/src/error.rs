// rust_websocket_utils/src/error.rs

//! 定义 WebSocket 工具库相关的错误类型。
//!
//! 本模块旨在提供一套清晰、可扩展的错误枚举，用于表示在 `rust_websocket_utils` 库的
//! 客户端和服务器端操作中可能发生的各种故障情况。通过使用 `thiserror` crate，
//! 我们可以方便地为每个错误变体定义用户友好的描述信息，并且支持错误的源链 (source chaining)。
//! 
//! 主要错误类型 `WsError` 整合了多种可能的错误来源，包括但不限于：
//! - 序列化/反序列化问题 (通常与 `serde_json` 相关)
//! - 底层的 WebSocket 协议错误 (来自 `tokio-tungstenite` 库)
//! - I/O 错误 (来自 `std::io`)
//! - URL 解析错误
//! - 连接状态问题 (如超时、未连接)
//! - 消息传输过程中的问题 (如通道关闭)

use thiserror::Error; // 引入 thiserror 来简化错误类型的定义 (规则 1.2, 5.1)
// use tokio_tungstenite::tungstenite::Error as TungsteniteError; // WebSocket 底层库的错误类型 - 已在 WsError 中通过 #[from] 引入
// use serde_json; // For WsClientError and potentially WsServerError - 已在 WsError 中通过 String 承载错误信息

// 下面的 WsUtilError, WsServerError, WsClientError 是旧的错误类型，已被注释掉或删除
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
///
/// 此枚举封装了在使用 `rust_websocket_utils` 库时可能遇到的各种错误情况。
/// 每个变体都提供了具体的错误上下文，并通过 `thiserror` 派生了 `Error` 和 `Debug` trait。
#[derive(Error, Debug)]
pub enum WsError {
    /// 当 `serde` 序列化（例如，将结构体转换为 JSON 字符串）失败时返回。
    /// 内部存储了描述序列化失败原因的字符串。
    #[error("数据序列化失败: {0}")] // 中文错误信息
    SerializationError(String),

    /// 当 `serde` 反序列化（例如，从 JSON 字符串解析为结构体）失败时返回。
    /// 内部存储了描述反序列化失败原因的字符串。
    #[error("数据反序列化失败: {0}")] // 中文错误信息
    DeserializationError(String),

    /// 表示在 WebSocket 协议层面发生的错误。
    /// 这通常是由底层的 `tokio-tungstenite` 库报告的错误，例如连接握手失败、
    /// 无效的 WebSocket 帧、连接被意外关闭等。
    /// 使用 `#[from]` 属性可以直接将 `tokio_tungstenite::tungstenite::Error` 转换为此变体。
    #[error("WebSocket 协议层错误: {0}")] // 中文错误信息
    WebSocketProtocolError(#[from] tokio_tungstenite::tungstenite::Error),

    /// 表示在执行底层输入/输出 (I/O) 操作时发生的错误。
    /// 例如，网络连接无法建立、读取或写入 TCP 流失败等。
    /// 使用 `#[from]` 属性可以直接将 `std::io::Error` 转换为此变体。
    #[error("网络或文件 I/O 错误: {0}")] // 中文错误信息
    IoError(#[from] std::io::Error),

    /// 当尝试通过一个已经关闭的 MPSC (多生产者单消费者) 通道发送消息时发生。
    /// 这通常表明接收端已经被丢弃或任务已结束。
    #[error("消息发送失败: 目标通道已关闭")] // 中文错误信息
    SendErrorClosed,

    /// 当尝试通过 WebSocket 连接发送消息，但对方（接收端）已经断开连接时发生。
    /// 错误信息中可能包含更详细的上下文。
    #[error("消息发送失败: 远程连接已断开 ({0})")] // 中文错误信息
    SendErrorDisconnected(String),

    /// 表示操作因超时而失败。
    /// 例如，等待 WebSocket 连接建立或等待响应超时。
    #[error("操作超时")] // 中文错误信息
    ConnectionTimeout,

    /// 当提供的 URL 字符串格式无效或无法被成功解析时发生。
    /// 错误信息中包含导致解析失败的具体原因。
    #[error("无效的 URL 格式: {0}")] // 中文错误信息
    InvalidUrl(String),

    /// 当尝试在一个尚未建立或已断开的 WebSocket 连接上执行需要连接的操作时发生。
    #[error("操作失败: 当前未连接到 WebSocket 服务器")] // 中文错误信息
    NotConnected,

    /// 一个通用的消息处理错误，用于其他未被更具体错误变体覆盖的情况。
    /// 错误信息中应包含对错误的简要描述。
    #[error("消息处理错误: {0}")] // 中文错误信息
    Message(String),
}

// 移除最后的占位注释
// // 暂时为空，后续会根据开发步骤添加具体错误枚举或结构。 