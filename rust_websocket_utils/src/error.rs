// rust_websocket_utils/src/error.rs

//! 定义 WebSocket 工具库相关的错误类型。

use thiserror::Error; // 引入 thiserror 来简化错误类型的定义 (规则 1.2, 5.1)
use tokio_tungstenite::tungstenite::Error as TungsteniteError; // WebSocket 底层库的错误类型

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
    HandshakeError(#[from] TungsteniteError),

    /// 当消息序列化 (例如转为 JSON) 失败时发生。
    /// (虽然在此模块中不直接使用，但作为通用错误类型包含进来，以供将来使用，比如发送 WsMessage)
    #[error("消息序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// 当发送 WebSocket 消息失败时发生。
    /// 注意：TungsteniteError 包含多种情况，这里概括性地表示发送错误。
    #[error("消息发送错误: {0}")]
    MessageSendError(TungsteniteError),

    /// 当接收 WebSocket 消息失败时发生。
    #[error("消息接收错误: {0}")]
    MessageReceiveError(TungsteniteError),

    /// 表示 WebSocket 连接已关闭。
    /// TungsteniteError::ConnectionClosed 通常用于此情况。
    #[error("连接已关闭")]
    ConnectionClosed,

    /// 通用 I/O 错误，用于其他未明确分类的 I/O 问题。
    #[error("I/O 错误: {0}")]
    GenericIoError(String),
}

// 暂时为空，后续会根据开发步骤添加具体错误枚举或结构。 