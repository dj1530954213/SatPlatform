// SatOnSiteMobile/src-tauri/src/event.rs

//! 定义 `SatOnSiteMobile` (现场端) 应用中 Rust 后端与前端 JavaScript/TypeScript 之间通信所使用的 Tauri 事件。
//!
//! 这些事件用于通知前端关于 WebSocket 连接状态、收到的消息、以及其他重要的后端状态变更。

use serde::{Serialize, Deserialize};
// 引入 ClientRole 以便在事件 payload 中使用强类型，如果适用
// use common_models::enums::ClientRole;

/// (已废弃/旧事件) 云端 WebSocket 连接成功事件的名称。
/// 建议使用 `WS_CONNECTION_STATUS_EVENT` 并检查其 `connected` 字段。
pub const EVENT_CLOUD_WS_CONNECTED: &str = "cloud-ws-connected";

/// (已废弃/旧事件) 云端 WebSocket 连接失败事件的名称。
/// 建议使用 `WS_CONNECTION_STATUS_EVENT` 并检查其 `connected` 字段和 `error_message`。
pub const EVENT_CLOUD_WS_CONNECT_FAILED: &str = "cloud-ws-connect-failed";

/// WebSocket 连接状态事件的名称。
///
/// 当 WebSocket 连接建立、断开或发生错误时，后端会发送此事件。
pub const WS_CONNECTION_STATUS_EVENT: &str = "ws_connection_status";

/// `WS_CONNECTION_STATUS_EVENT` 事件的 Payload 结构体。
#[derive(Clone, Serialize, Debug)]
pub struct WsConnectionStatusEvent {
    /// 表示当前是否已连接到 WebSocket 服务器。
    /// `true` 表示已连接, `false` 表示未连接或已断开。
    pub connected: bool,
    /// 如果连接成功，可能包含由云端分配的客户端唯一 ID。
    pub client_id: Option<String>,
    /// 如果连接失败或断开，可能包含相关的错误信息或原因。
    pub error_message: Option<String>,
}

// --- Echo (回声测试) 相关事件 --- 

/// Echo (回声) 响应事件的名称。
///
/// 当后端收到来自云端的 Echo 消息回复时，会发送此事件，并将 Echo 内容传递给前端。
pub const ECHO_RESPONSE_EVENT: &str = "echo_response_event";

/// `ECHO_RESPONSE_EVENT` 事件的 Payload 结构体。
#[derive(Clone, Serialize, serde::Deserialize, Debug)] // 添加 Deserialize 以便潜在的测试或双向通信
pub struct EchoResponseEventPayload {
    /// Echo 响应的具体内容字符串。
    pub content: String,
}

// --- 客户端注册与任务状态同步相关事件 (P4.1.1 及后续) ---

/// WebSocket 客户端注册状态事件的名称。
///
/// 当客户端尝试向云端注册（例如，通过 `register_client_with_task` 命令发送 "Register" 消息后），
/// 云端会回复一个 "RegisterResponse" 消息，后端处理后通过此事件将注册结果通知给前端。
pub const WS_REGISTRATION_STATUS_EVENT: &str = "ws_registration_status_event";

/// `WS_REGISTRATION_STATUS_EVENT` 事件的 Payload 结构体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRegistrationStatusEventPayload {
    /// 指示注册是否成功。
    pub success: bool,
    /// 如果注册失败，可能包含失败的原因或相关消息。
    pub message: Option<String>,
    /// 如果注册成功，云端分配给客户端的唯一ID (重要！)
    pub assigned_client_id: Option<String>,
    /// 如果注册成功，可能包含客户端成功加入的组 ID。
    pub group_id: Option<String>,
    /// 如果注册成功，可能包含与当前会话关联的任务 ID。
    pub task_id: Option<String>,
}

/// WebSocket 伙伴客户端状态更新事件的名称。
///
/// 当同一任务组内的伙伴客户端上线或下线时，云端会通知组内其他成员，
/// 后端收到此通知后，通过此事件将伙伴的状态变更情况传递给前端。
pub const WS_PARTNER_STATUS_EVENT: &str = "ws_partner_status_event";

/// `WS_PARTNER_STATUS_EVENT` 事件的 Payload 结构体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPartnerStatusEventPayload {
    /// 发生状态变更的伙伴客户端的角色。
    /// 通常是 `common_models::enums::ClientRole` 枚举值的字符串表示形式，
    /// 例如："ControlCenter", "OnSiteMobile"。
    pub partner_role: String, 
    // pub partner_role: ClientRole, // 如果希望使用强类型且前端能方便处理
    
    /// 指示伙伴客户端当前是否在线。
    /// `true` 表示在线, `false` 表示离线。
    pub is_online: bool,
}

/// 本地任务调试状态更新事件的名称。
///
/// 当客户端（无论是自身操作导致还是收到伙伴客户端操作的通知）的本地权威任务状态
/// (`TaskDebugState`) 发生更新时，后端服务会通过此事件将更新后的完整状态发送给前端 UI。
/// 前端应监听此事件来刷新显示任务相关的界面。
pub const LOCAL_TASK_STATE_UPDATED_EVENT: &str = "local_task_state_updated_event";

/// `LOCAL_TASK_STATE_UPDATED_EVENT` 事件的 Payload 结构体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTaskStateUpdatedEventPayload {
    /// 更新后的完整任务调试状态。
    /// 使用 `serde_json::Value` 类型以灵活地表示复杂的、可能嵌套的 `TaskDebugState` 结构。
    /// 前端在接收时，应能将此 `Value` 解析回其 TypeScript 中对应的 `TaskDebugState` 接口/类。
    pub new_state: serde_json::Value, 
    // 或者，如果 TaskDebugState 在 common_models 中定义并在此处直接引用：
    // pub new_state: common_models::task_state::TaskDebugState, 
    // (前提是 common_models::task_state::TaskDebugState 实现了 Serialize 和 Deserialize)
} 