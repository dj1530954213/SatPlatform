use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::enums::ClientRole; // 确保 common_models/src/enums.rs 中有 ClientRole
use chrono::Utc;

// 预检查项的状态
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PreCheckItemState {
    pub item_id: String,
    pub status_from_site: Option<String>, // 例如 "Pending", "Site_Completed", "Site_Failed"
    pub notes_from_site: Option<String>,
    pub status_from_control: Option<String>, // 例如 "Pending", "Confirmed", "Rejected"
    pub notes_from_control: Option<String>,
    pub last_updated: i64, // 时间戳 (Unix epoch milliseconds)
}

impl PreCheckItemState {
    /// 创建一个新的 PreCheckItemState 实例。
    pub fn new(item_id: String) -> Self {
        Self {
            item_id,
            status_from_site: None,
            notes_from_site: None,
            status_from_control: None,
            notes_from_control: None,
            last_updated: Utc::now().timestamp_millis(),
        }
    }
}

// 单体测试步骤的状态
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleTestStepState {
    pub step_id: String,
    pub device_id: String,
    pub command_from_control: Option<String>, // 中心端下发的指令
    pub execution_status_from_site: Option<String>, // 例如 "Pending", "Executing", "Completed", "Failed"
    pub result_data_from_site: Option<serde_json::Value>, // 现场端返回的结果数据
    pub feedback_notes_from_site: Option<String>, // 现场端反馈的备注
    pub confirmation_status_from_control: Option<String>, // 例如 "Pending", "Confirmed", "Rejected"
    pub last_updated: i64, // 时间戳 (Unix epoch milliseconds)
}

impl SingleTestStepState {
    /// 创建一个新的 SingleTestStepState 实例。
    pub fn new(step_id: String, device_id: String) -> Self {
        Self {
            step_id,
            device_id,
            command_from_control: None,
            execution_status_from_site: None,
            result_data_from_site: None,
            feedback_notes_from_site: None,
            confirmation_status_from_control: None,
            last_updated: Utc::now().timestamp_millis(),
        }
    }
}

/// 调试任务的整体共享状态模型。
///
/// 此结构体在云端完整地表示一个正在进行的调试任务的全部共享状态。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskDebugState {
    /// 任务的唯一标识符。
    pub task_id: String,
    /// 预检查项的状态集合，键为 `item_id`。
    pub pre_check_items: HashMap<String, PreCheckItemState>,
    /// 单体测试步骤的状态集合，键通常为 `step_id` 或 `device_id`+`step_id`。
    pub single_test_steps: HashMap<String, SingleTestStepState>,
    // TODO: 未来可能还有其他调试环节的状态，例如联锁条件等
    // pub interlocking_conditions: HashMap<String, InterlockConditionState>,
    /// 最后更新此状态的客户端角色。
    pub last_updated_by_role: Option<ClientRole>,
    /// 最后更新的时间戳 (Unix epoch milliseconds)。
    pub last_update_timestamp: i64,
}

impl TaskDebugState {
    /// 根据任务 ID 创建一个新的 `TaskDebugState` 实例。
    ///
    /// 初始化时，状态集合为空，时间戳设置为当前时间。
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            pre_check_items: HashMap::new(),
            single_test_steps: HashMap::new(),
            // interlocking_conditions: HashMap::new(),
            last_updated_by_role: None,
            last_update_timestamp: Utc::now().timestamp_millis(),
        }
    }
}

// --- 业务 Payloads ---
// 以下是客户端发起、意图改变共享任务状态或报告状态的业务消息 Payload 示例。

/// 更新预检查项状态的 Payload。
///
/// 由客户端（现场端或中心端）发送，用于更新特定预检查项的状态和备注。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdatePreCheckItemPayload {
    pub task_id: String,
    pub item_id: String,
    /// 具体的状态值，例如 "Site_Completed", "Confirmed"。
    pub status: String,
    pub notes: Option<String>,
    // 注意: `updated_by_role` 通常由服务器根据发送消息的客户端会话角色来确定，
    // 而不是在此 Payload 中由客户端直接指定。
}

/// 发起单体测试步骤的 Payload。
///
/// 通常由中心端发送，用于指令现场端执行某个设备的特定测试步骤。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StartSingleTestStepPayload {
    pub task_id: String,
    pub device_id: String,
    pub step_id: String,
    /// 具体指令内容，例如 "RUN_FORWARD_5_SEC"。
    pub command: String,
}

/// 反馈单体测试步骤结果的 Payload。
///
/// 通常由现场端发送，用于报告单体测试步骤的执行状态、结果数据和备注。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FeedbackSingleTestStepPayload {
    pub task_id: String,
    pub device_id: String,
    pub step_id: String,
    /// 执行状态，例如 "Completed", "Failed"。
    pub execution_status: String,
    /// 测试结果数据，可以是任意 JSON 值。
    pub result_data: Option<serde_json::Value>,
    pub feedback_notes: Option<String>,
}

/// 确认单体测试步骤的 Payload。
///
/// 通常由中心端发送，用于确认现场端反馈的单体测试步骤结果。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfirmSingleTestStepPayload {
    pub task_id: String,
    pub device_id: String,
    pub step_id: String,
    /// 确认状态，例如 "Confirmed", "Rejected"。
    pub confirmation_status: String,
} 