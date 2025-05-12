// SatOnSiteMobile/src-tauri/src/events.rs
// 内容与 SatControlCenter/src-tauri/src/events.rs 基本相同
// Tauri 事件是全局的，如果两端应用需要区分事件来源或有不同处理，
// 可以在事件 payload 中加入更多上下文，或者定义不同的事件名称。
// 为了P4.1.1的最小实现，我们复用相同的事件结构和名称。

use common_models::{enums::ClientRole, task_state::TaskDebugState};
use serde::Serialize;
use log::{error, info, warn, debug}; // 添加日志导入

/// WebSocket 注册状态事件
#[derive(Clone, Debug, Serialize)]
pub struct WsRegistrationStatusEvent {
    pub success: bool,
    pub message: Option<String>,
    pub group_id: Option<String>,
    pub task_id: Option<String>,
}

/// 伙伴状态更新事件
#[derive(Clone, Debug, Serialize)]
pub struct WsPartnerStatusEvent {
    pub partner_role: ClientRole,
    pub is_online: bool,
    pub group_id: String,
}

/// 本地任务状态更新事件
#[derive(Clone, Debug, Serialize)]
pub struct LocalTaskStateUpdatedEvent {
    #[serde(rename = "newState")]
    pub new_state: TaskDebugState,
}

pub const EVENT_WS_REGISTRATION_STATUS: &str = "event://ws-registration-status";
pub const EVENT_WS_PARTNER_STATUS: &str = "event://ws-partner-status";
pub const EVENT_LOCAL_TASK_STATE_UPDATED: &str = "event://local-task-state-updated";


pub fn emit_tauri_event<S: Serialize + Clone>(
    app_handle: &tauri::AppHandle,
    event_name: &str,
    payload: S,
) {
    info!("准备发射 Tauri 事件 '{}' (SatOnSiteMobile)", event_name);
    if let Err(e) = app_handle.emit_all(event_name, payload) {
        error!("发射 Tauri 事件 '{}' 失败 (SatOnSiteMobile): {}", event_name, e);
    } else {
        debug!("Tauri 事件 '{}' 发射成功 (SatOnSiteMobile)", event_name);
    }
} 