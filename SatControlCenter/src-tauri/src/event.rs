// SatControlCenter/src-tauri/src/event.rs

//! 定义 Tauri 事件，用于 Rust 与前端的通信。

use serde::Serialize;

/// 云端 WebSocket 连接成功事件
pub const EVENT_CLOUD_WS_CONNECTED: &str = "cloud-ws-connected";
/// 云端 WebSocket 连接失败事件
pub const EVENT_CLOUD_WS_CONNECT_FAILED: &str = "cloud-ws-connect-failed";
// 后续可扩展更多事件，如消息接收、断开等。

// 暂时为空，后续会根据需要定义具体的事件常量或结构。
// 例如： pub const EVENT_WS_CONNECTED: &str = "event-ws-connected"; 

// WebSocket 连接状态事件
// 当 WebSocket 连接状态发生变化时（成功连接、断开连接、发生错误），由此事件通知前端。
#[derive(Clone, Serialize, Debug)]
pub struct WsConnectionStatusEvent {
    pub connected: bool,                    // 是否已连接
    pub client_id: Option<String>,          // 云端分配的 client_id (连接成功时尚未分配则为 None)
    pub error_message: Option<String>,      // 如果连接失败或断开时，包含错误信息
}

// 此常量用于在前端监听事件时使用，例如：
// import { listen } from '@tauri-apps/api/event';
// await listen(WS_CONNECTION_STATUS_EVENT, (event) => {
//   console.log('Connection Status:', event.payload);
// });
pub const WS_CONNECTION_STATUS_EVENT: &str = "ws_connection_status";

// --- Echo 响应事件 (P2.2.1) ---
/// Echo 响应事件的名称常量
pub const ECHO_RESPONSE_EVENT: &str = "echo_response_event";

/// Echo 响应事件的 Payload 结构体
/// 当从云端收到 Echo 回复时，由此事件将内容通知给前端。
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)] // 添加 Deserialize 以便与现场端一致
pub struct EchoResponseEventPayload {
    pub content: String,
}

// 后续可以根据需要在此文件定义更多应用级的 Tauri 事件结构体 