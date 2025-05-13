// SatControlCenter/src-tauri/src/event.rs

//! `SatControlCenter` (卫星控制中心) 应用事件定义模块。
//!
//! 本模块集中定义了所有用于 Tauri 后端 (Rust) 与前端 (Angular) 之间进行异步通信的事件。
//! 这些事件主要通过 Tauri 的事件系统 (`emit` 和 `listen` API) 进行发送和接收。
//! 定义的内容包括：
//! - **事件名称常量**: 字符串常量，作为事件的唯一标识符，在前端监听和后端发送时使用。
//! - **事件负载结构体**: 当事件需要传递复杂数据时，会定义相应的 Rust 结构体。
//!   这些结构体通常会派生 `serde::Serialize` 以便能够被序列化为 JSON 并发送给前端，
//!   同时也会派生 `Clone` 和 `Debug` 以方便使用。

use serde::{Deserialize, Serialize}; // 引入 Serialize trait，用于将 Rust 结构体序列化为 JSON 等格式，以便在 Tauri 事件中传递给前端。
use common_models::{TaskDebugState};

// --- 云端 WebSocket 连接状态相关事件名称常量 --- 

/// 事件名称常量：表示已成功连接到云端 WebSocket 服务。
/// 当 `ws_client::WebSocketClientService` 成功与 `SatCloudService` 建立 WebSocket 连接后，
/// 可能会触发此事件，通知前端连接已就绪。
pub const EVENT_CLOUD_WS_CONNECTED: &str = "cloud-ws-connected";

/// 事件名称常量：表示尝试连接到云端 WebSocket 服务失败。
/// 当 `ws_client::WebSocketClientService` 尝试连接 `SatCloudService` 但未能成功时
/// (例如，网络问题、服务端未运行、URL错误等)，可能会触发此事件，通知前端连接尝试已失败。
/// 通常会伴随一个包含错误信息的负载 (例如 `WsConnectionStatusEvent`)。
pub const EVENT_CLOUD_WS_CONNECT_FAILED: &str = "cloud-ws-connect-failed";

// 提示：后续可以根据具体业务需求，在此处扩展更多与云端 WebSocket 通信相关的特定事件，
// 例如：收到特定类型的业务消息、连接意外断开、认证成功/失败等。

// 占位注释：以下为早前构思的事件常量，当前可能已被更具体的事件 (如 WS_CONNECTION_STATUS_EVENT) 所覆盖或整合。
// 根据实际开发进展，可以考虑移除或重新评估其必要性。
// 例如： pub const EVENT_WS_CONNECTED: &str = "event-ws-connected"; 

// --- WebSocket 详细连接状态变更事件 --- 

/// WebSocket 连接状态事件 (`ws_connection_status`) 的负载。
///
/// 当 `ws_client::WebSocketClientService` 尝试连接 `SatCloudService` 或连接状态发生改变时，
/// 由后端服务发出此事件，通知前端当前的连接状态。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsConnectionStatusEvent {
    /// 指示是否已连接。
    pub connected: bool,
    /// 如果连接成功，云端分配的客户端 ID (UUID 字符串)。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// 如果连接失败或断开，相关的错误信息或原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// `WsConnectionStatusEvent` (WebSocket 连接状态事件) 的标准事件名称常量。
///
/// 后端 (Rust) 在发送 `WsConnectionStatusEvent` (WebSocket 连接状态事件) 数据时，应使用此常量作为事件名称。
/// 前端 (Angular) 在监听此类事件时，也应使用此常量。
///
/// 前端 (TypeScript/Angular) 监听此事件的示例代码片段：
/// ```typescript
/// import {{ listen, Event }} from '@tauri-apps/api/event';
/// 
/// interface WsConnectionStatusPayload {{
///   connected: boolean;
///   client_id?: string;
///   error_message?: string;
/// }}
/// 
/// async function setupWsStatusListener() {{
///   await listen<WsConnectionStatusPayload>('{WS_CONNECTION_STATUS_EVENT}', (event: Event<WsConnectionStatusPayload>) => {{
///     console.log('WebSocket 连接状态发生变化:', event.payload);
///     if (event.payload.connected) {{
///       console.log('已连接到云端，客户端ID:', event.payload.client_id || '尚未分配');
///       // TODO: 更新 UI 显示为"已连接"，并处理 client_id
///     }} else {{
///       console.warn('已从云端断开或连接失败。错误信息:', event.payload.error_message || '无特定错误信息');
///       // TODO: 更新 UI 显示为"已断开"，并提示错误信息
///     }}
///   }});
///   console.log('已启动 WebSocket 连接状态事件监听器。');
/// }}
/// 
/// setupWsStatusListener();
/// ```
pub const WS_CONNECTION_STATUS_EVENT: &str = "ws_connection_status";

// --- Echo (回声测试) 响应事件 (项目阶段 P2.2.1 引入) --- 

/// Echo 测试响应事件 (`echo_response_event`) 的负载。
///
/// 当 `ws_client::WebSocketClientService` 收到来自云端的 Echo 响应消息时发出。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EchoResponseEventPayload {
    /// Echo 的内容。
    pub content: String,
}

/// `EchoResponseEventPayload` (Echo响应事件负载) 的标准事件名称常量。
///
/// 当 `SatControlCenter` 的后端通过 WebSocket 向云端服务发送了一个 Echo (回声) 请求，
/// 并且成功收到了云端的回声响应后，后端将使用此事件名称，
/// 将包含原始响应内容的 `EchoResponseEventPayload` (Echo响应事件负载) 发送给前端。
pub const ECHO_RESPONSE_EVENT: &str = "echo_response_event";

// 提示：后续可以根据 `SatControlCenter` 应用的特定业务需求，在此文件中定义更多应用级的 Tauri 事件常量和负载结构体。
// 例如，当 PLC 通信状态发生变化、或特定控制指令执行完成时，都可以定义相应的事件来通知前端。 

/// WebSocket 注册状态事件 (`ws_registration_status`) 的负载。
///
/// 当 WebSocket 客户端尝试向云端注册并收到响应时，由后端服务发出此事件，
/// 通知前端注册操作的结果。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsRegistrationStatusEventPayload {
    /// 指示注册是否成功。
    pub success: bool,
    /// 可选的附加信息，例如成功消息或失败原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 客户端尝试注册或已成功注册到的组 ID。改为 Option<String> 以匹配 service.rs 用法。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// 关联的任务 ID。新增字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 如果注册成功，服务器分配给此客户端的唯一ID (UUID 字符串)。改为 Option<String>。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_client_id: Option<String>,
}

pub const WS_REGISTRATION_STATUS_EVENT: &str = "ws_registration_status";

/// WebSocket 伙伴状态事件 (`ws_partner_status`) 的负载。
///
/// 当云端通知有伙伴（例如现场端）上线或下线时，由后端服务发出此事件，
/// 通知前端伙伴的状态变化。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsPartnerStatusEventPayload {
    /// 发生状态变化的伙伴的角色 (字符串形式)。
    pub partner_role: String,
    /// 指示伙伴是否在线 (true 表示上线/加入组，false 表示下线/离开组)。
    pub is_online: bool,
}

pub const WS_PARTNER_STATUS_EVENT: &str = "ws_partner_status";

/// 本地任务状态更新事件 (`local_task_state_updated`) 的负载。
///
/// 当 `ws_client::WebSocketClientService` 从云端收到 `TaskStateUpdate` 消息并更新了
/// 本地缓存的 `TaskDebugState` 后，发出此事件，将最新的完整状态通知给前端。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalTaskStateUpdatedEventPayload {
    /// 从云端接收到的、最新的完整任务调试状态。
    pub new_state: TaskDebugState,
}

pub const LOCAL_TASK_STATE_UPDATED_EVENT: &str = "local_task_state_updated";

// --- (可选) 服务端错误事件 ---

/// 服务端错误事件 (`ws_server_error`) 的负载。
///
/// 当 `ws_client::WebSocketClientService` 收到来自云端的明确错误响应消息
/// (例如 `ErrorResponse` 类型) 时，可以发出此事件通知前端。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsServerErrorEventPayload {
    /// 原始导致错误的请求消息类型（如果可用）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_message_type: Option<String>,
    /// 服务端返回的错误描述信息。
    pub error: String,
}

pub const WS_SERVER_ERROR_EVENT: &str = "ws_server_error"; 