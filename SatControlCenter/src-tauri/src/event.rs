// SatControlCenter/src-tauri/src/event.rs

//! 定义 `SatControlCenter` (中心端) Rust 后端与前端 JavaScript/TypeScript 之间进行异步通信所使用的 Tauri 事件。
//!
//! 本模块集中定义了所有事件的名称常量和相关的负载 (payload) 结构体。
//! 这些事件旨在通知前端关于 WebSocket 连接状态的变更、从云端服务接收到的特定消息、
//! 以及其他重要的后端状态或操作结果。

use serde::{Serialize, Deserialize};
// use common_models::{self, TaskDebugState, enums::ClientRole}; // ClientRole is unused if partner_role is String
use common_models::{self, TaskDebugState}; // Ensure TaskDebugState is correctly imported
// use uuid::Uuid; // Uuid is unused if client_id fields are String

/// WebSocket 连接状态变更事件的统一名称常量。
///
/// 当 `SatControlCenter` 应用的 WebSocket 连接状态发生任何变化时（例如：成功连接、连接断开、连接尝试失败等），
/// 后端服务会发出此事件，并附带一个 `WsConnectionStatusEvent` 作为负载，详细说明当前状态。
pub const WS_CONNECTION_STATUS_EVENT: &str = "ws_connection_status";

/// `WS_CONNECTION_STATUS_EVENT` 事件的负载结构体。
///
/// 封装了 WebSocket 连接状态的详细信息。
#[derive(Clone, Serialize, Debug)] // 注意：如果前端也可能发送此结构或用于反序列化，则应添加 Deserialize
pub struct WsConnectionStatusEvent {
    /// 指示当前是否已成功连接到 WebSocket 服务器。
    /// - `true`: 表示已连接。
    /// - `false`: 表示未连接或连接已断开/失败。
    pub connected: bool,
    /// 如果连接成功 (`connected` 为 `true`)，此字段可能包含由云端服务分配给当前客户端的唯一标识符 (UUID 字符串格式)。
    /// 如果未连接或连接失败，则为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// 如果连接失败或在连接过程中发生错误，亦或连接意外断开时，此字段可能包含相关的错误描述信息或断开原因。
    /// 如果连接成功且无错误，则为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

// --- Echo (回声测试) 相关事件 --- 

/// Echo (回声) 响应消息事件的名称常量。
///
/// 当后端服务从云端 WebSocket 服务收到一个 Echo 类型的响应消息后，
/// 会通过此事件名称将 Echo 内容转发给前端。
pub const ECHO_RESPONSE_EVENT: &str = "echo_response_event";

/// `ECHO_RESPONSE_EVENT` 事件的负载结构体。
///
/// 包含从云端 Echo 服务返回的具体内容。
#[derive(Clone, Serialize, Deserialize, Debug)] // 添加 Deserialize 以便潜在的测试或双向通信场景
pub struct EchoResponseEventPayload {
    /// Echo 响应的具体内容字符串。
    pub content: String,
}

// --- 客户端注册与任务状态同步相关事件 (P4.1.1 及后续阶段引入) ---

/// WebSocket 客户端向云端服务注册的结果状态事件的名称常量。
///
/// 当中心端应用尝试通过 WebSocket 向云端服务注册（例如，加入一个任务组）并收到云端的响应后，
/// 后端服务会发出此事件，将注册操作的结果（成功或失败及其原因）通知给前端。
pub const WS_REGISTRATION_STATUS_EVENT: &str = "ws_registration_status_event";

/// `WS_REGISTRATION_STATUS_EVENT` 事件的负载结构体。
///
/// 封装了客户端注册尝试的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRegistrationStatusEventPayload {
    /// 指示注册尝试是否成功。
    /// - `true`: 注册成功。
    /// - `false`: 注册失败。
    pub success: bool,
    /// 如果注册失败 (`success` 为 `false`)，此字段可能包含描述失败原因的文本消息。
    /// 如果注册成功，则通常为 `None`，或包含一条成功的提示信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 如果注册成功，此字段包含由云端服务分配给当前客户端的唯一标识符 (UUID 字符串格式)。
    /// 这是客户端在后续通信中非常重要的身份凭证。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_client_id: Option<String>,
    /// 如果注册成功且操作涉及加入某个组，此字段可能包含客户端成功加入的组的 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// 如果注册成功且操作涉及特定任务，此字段可能包含与当前会话关联的任务 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 如果注册成功且操作涉及特定角色，此字段可能包含客户端的角色。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// WebSocket 伙伴客户端状态更新事件的名称常量。
///
/// 当同一任务组内的伙伴客户端（例如，与之协作的现场端）的在线状态发生变化（上线或下线）时，
/// 云端服务会通知组内其他成员。中心端后端收到此通知后，通过此事件将伙伴的状态变更情况传递给前端。
pub const WS_PARTNER_STATUS_EVENT: &str = "ws_partner_status_event";

/// `WS_PARTNER_STATUS_EVENT` 事件的负载结构体。
///
/// 包含伙伴客户端状态变化的详细信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPartnerStatusEventPayload {
    /// 发生状态变更的伙伴客户端的角色。
    /// 通常是 `common_models::enums::ClientRole` 枚举值的字符串表示形式，例如："ControlCenter", "OnSiteMobile"。
    pub partner_role: String, 
    // pub partner_role: common_models::enums::ClientRole, // 备选：如果希望在事件负载中使用强类型的角色枚举，并确保前端能方便处理。
    
    /// 指示伙伴客户端当前是否在线。
    /// - `true`: 表示伙伴客户端当前在线并已加入任务组。
    /// - `false`: 表示伙伴客户端已离线或已离开任务组。
    pub is_online: bool,
    /// 发生状态变化的伙伴客户端的唯一标识符 (UUID 字符串格式)。
    /// 用于在前端精确识别是哪个伙伴的状态发生了变化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partner_client_id: Option<String>,
    /// 相关的任务组 ID，指明是哪个组内的伙伴状态发生了变化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

/// 本地缓存的任务调试状态更新事件的名称常量。
///
/// 当客户端（无论是由于自身操作触发，还是收到来自云端的伙伴客户端操作的通知）
/// 的本地权威任务状态 (`common_models::TaskDebugState`) 发生更新时，
/// 后端服务会通过此事件将更新后的完整任务状态对象发送给前端 UI。
/// 前端应监听此事件以实时刷新显示任务相关的界面元素。
pub const LOCAL_TASK_STATE_UPDATED_EVENT: &str = "local_task_state_updated_event";

/// `LOCAL_TASK_STATE_UPDATED_EVENT` 事件的负载结构体。
///
/// 包含更新后的完整任务调试状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTaskStateUpdatedEventPayload {
    /// 更新后的完整任务调试状态 (`TaskDebugState`)。
    pub new_state: TaskDebugState, 
}

// --- 云端错误响应事件 ---
pub const WS_SERVER_ERROR_EVENT: &str = "ws_server_error_event";

/// `WS_SERVER_ERROR_EVENT` 事件的负载结构体。
/// 用于将云端通过 ErrorResponsePayload 返回的错误信息传递给前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsServerErrorEventPayload {
    pub error_message: String, // 来自 ErrorResponsePayload.error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_message_type: Option<String>, // 来自 ErrorResponsePayload.original_message_type
}

// WebSocket Server (Cloud) Connection Events
pub const EVENT_CLOUD_WS_DISCONNECTED: &str = "cloud-ws-disconnected";
pub const EVENT_CLOUD_WS_ERROR: &str = "cloud-ws-error"; 