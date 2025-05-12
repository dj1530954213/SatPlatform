use common_models::{enums::ClientRole, task_state::TaskDebugState};
use serde::Serialize;
use log::{error, info, warn, debug};

/// WebSocket 注册状态事件
/// 当客户端尝试向云端注册后，通过此事件通知前端注册结果。
#[derive(Clone, Debug, Serialize)]
pub struct WsRegistrationStatusEvent {
    /// 指示注册是否成功。
    pub success: bool,
    /// 包含成功或失败的附加信息。
    pub message: Option<String>,
    /// 如果注册成功，这里是客户端最终加入的组ID。
    pub group_id: Option<String>,
    /// 客户端尝试注册时关联的任务ID，方便前端进行状态跟踪。
    pub task_id: Option<String>, // 注意：此 task_id 的填充依赖于前端或调用命令时的上下文
}

/// 伙伴状态更新事件
/// 当同组内的伙伴客户端上线或下线时，通过此事件通知前端。
#[derive(Clone, Debug, Serialize)]
pub struct WsPartnerStatusEvent {
    /// 发生状态变化的伙伴的角色。
    pub partner_role: ClientRole,
    /// 指示伙伴是上线 (true) 还是下线 (false)。
    pub is_online: bool,
    /// 相关的组ID。
    pub group_id: String,
}

/// 本地任务状态更新事件
/// 当从云端接收到最新的任务调试状态 (TaskDebugState) 并更新本地缓存后，通过此事件通知前端。
#[derive(Clone, Debug, Serialize)]
pub struct LocalTaskStateUpdatedEvent {
    /// 最新的任务调试状态。
    /// 使用 serde rename 来匹配前端可能期望的驼峰命名。
    #[serde(rename = "newState")]
    pub new_state: TaskDebugState,
}

// 事件名称常量，用于在 Rust 和 JavaScript/TypeScript 之间进行事件通信。
// 遵循 "event://<scope>/<name>" 或类似的命名约定有助于组织。
pub const EVENT_WS_REGISTRATION_STATUS: &str = "event://ws-registration-status";
pub const EVENT_WS_PARTNER_STATUS: &str = "event://ws-partner-status";
pub const EVENT_LOCAL_TASK_STATE_UPDATED: &str = "event://local-task-state-updated";

/// 辅助函数，用于发射 Tauri 事件。
/// 此函数对 app_handle.emit_all 进行了封装，增加了日志记录。
pub fn emit_tauri_event<S: Serialize + Clone>(
    app_handle: &tauri::AppHandle,
    event_name: &str,
    payload: S,
) {
    info!("准备发射 Tauri 事件 '{}'", event_name);
    if let Err(e) = app_handle.emit_all(event_name, payload) {
        error!("发射 Tauri 事件 '{}' 失败: {}", event_name, e);
    } else {
        debug!("Tauri 事件 '{}' 发射成功", event_name);
    }
} 