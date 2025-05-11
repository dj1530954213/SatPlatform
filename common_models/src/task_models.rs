use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::enums::ClientRole; // 确保 common_models/src/enums.rs 中有 ClientRole
use chrono::{DateTime, Utc};

// 预检查项的状态
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PreCheckItemStatus {
    pub item_id: String,
    pub status_from_site: Option<String>, // 例如 "Pending", "Site_Completed", "Site_Failed"
    pub notes_from_site: Option<String>,
    pub status_from_control: Option<String>, // 例如 "Pending", "Confirmed", "Rejected"
    pub notes_from_control: Option<String>,
    pub last_updated: DateTime<Utc>,
}

impl PreCheckItemStatus {
    /// 创建一个新的 PreCheckItemStatus 实例。
    pub fn new(item_id: String) -> Self {
        Self {
            item_id,
            status_from_site: None,
            notes_from_site: None,
            status_from_control: None,
            notes_from_control: None,
            last_updated: Utc::now(),
        }
    }
}

// 单体测试步骤的状态
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SingleTestStepStatus {
    pub step_id: String,
    pub command_from_control: Option<String>,
    pub params_from_control: Option<serde_json::Value>, // 灵活的参数
    pub execution_status_from_site: Option<String>, // "Pending", "Running", "Completed", "Failed"
    pub result_data_from_site: Option<serde_json::Value>,
    pub feedback_notes_from_site: Option<String>,
    pub confirmation_status_from_control: Option<String>, // "Pending", "Confirmed", "Rejected"
    pub last_updated: DateTime<Utc>,
}

impl SingleTestStepStatus {
    /// 创建一个新的 SingleTestStepStatus 实例。
    pub fn new(step_id: String) -> Self {
        Self {
            step_id,
            command_from_control: None,
            params_from_control: None,
            execution_status_from_site: None,
            result_data_from_site: None,
            feedback_notes_from_site: None,
            confirmation_status_from_control: None,
            last_updated: Utc::now(),
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
    pub pre_check_items: HashMap<String, PreCheckItemStatus>,
    /// 单体测试步骤的状态集合，键通常为 `step_id` 或 `device_id`+`step_id`。
    pub single_test_steps: HashMap<String, SingleTestStepStatus>,
    // TODO: 未来可能还有其他调试环节的状态，例如联锁条件等
    // pub interlocking_conditions: HashMap<String, InterlockConditionState>,
    /// 最后更新此状态的客户端角色。
    pub last_updated_by_role: Option<ClientRole>,
    /// 最后更新的时间戳 (Unix epoch milliseconds)。
    pub last_update_timestamp: DateTime<Utc>,
    pub version: u64, // 用于乐观锁或版本跟踪
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
            last_update_timestamp: Utc::now(),
            version: 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::ClientRole;
    use chrono::Utc;
    use serde_json;

    #[test]
    fn test_pre_check_item_status_serialization_deserialization() {
        let original_item_status = PreCheckItemStatus {
            item_id: "item_001".to_string(),
            status_from_site: Some("Site_Completed".to_string()),
            notes_from_site: Some("All good from site.".to_string()),
            status_from_control: Some("Confirmed".to_string()),
            notes_from_control: Some("Control confirms.".to_string()),
            last_updated: Utc::now(),
        };

        let serialized = serde_json::to_string(&original_item_status).unwrap();
        // println!("Serialized PreCheckItemStatus: {}", serialized); // 用于调试
        let deserialized: PreCheckItemStatus = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original_item_status.item_id, deserialized.item_id);
        assert_eq!(original_item_status.status_from_site, deserialized.status_from_site);
        // 比较 DateTime<Utc> 时，由于可能的微小精度差异，直接比较可能失败
        // 这里我们比较时间戳（例如，毫秒数）是否在可接受的误差范围内
        assert!((original_item_status.last_updated - deserialized.last_updated).num_milliseconds().abs() < 1000, "Timestamp mismatch too large");


        let minimal_item_status = PreCheckItemStatus::new("item_002".to_string());
        let serialized_minimal = serde_json::to_string(&minimal_item_status).unwrap();
        let deserialized_minimal: PreCheckItemStatus = serde_json::from_str(&serialized_minimal).unwrap();
        assert_eq!(minimal_item_status.item_id, deserialized_minimal.item_id);
        assert!(deserialized_minimal.status_from_site.is_none());
    }

    #[test]
    fn test_single_test_step_status_serialization_deserialization() {
        let original_step_status = SingleTestStepStatus {
            step_id: "step_abc".to_string(),
            command_from_control: Some("START_MOTOR".to_string()),
            params_from_control: Some(serde_json::json!({"speed": 100, "duration": 5})),
            execution_status_from_site: Some("Completed".to_string()),
            result_data_from_site: Some(serde_json::json!({"actual_duration": 5.1})),
            feedback_notes_from_site: Some("Motor ran smoothly".to_string()),
            confirmation_status_from_control: Some("Confirmed".to_string()),
            last_updated: Utc::now(),
        };

        let serialized = serde_json::to_string(&original_step_status).unwrap();
        let deserialized: SingleTestStepStatus = serde_json::from_str(&serialized).unwrap();
        
        // 使用 PartialEq 进行比较, DateTime<Utc> 的比较需要注意
        assert_eq!(original_step_status.step_id, deserialized.step_id);
        assert_eq!(original_step_status.command_from_control, deserialized.command_from_control);
        assert_eq!(original_step_status.params_from_control, deserialized.params_from_control);
        assert!((original_step_status.last_updated - deserialized.last_updated).num_milliseconds().abs() < 1000, "Timestamp mismatch too large for single test step");
    }
    
    #[test]
    fn test_task_debug_state_serialization_deserialization() {
        let mut original_state = TaskDebugState::new("task_xyz_123".to_string());
        original_state.last_updated_by_role = Some(ClientRole::ControlCenter);
        
        let pre_check_item1 = PreCheckItemStatus {
            item_id: "pc_001".to_string(),
            status_from_site: Some("Site_Completed".to_string()),
            notes_from_site: Some("Site check 1 done".to_string()),
            status_from_control: None,
            notes_from_control: None,
            last_updated: Utc::now(),
        };
        original_state.pre_check_items.insert("pc_001".to_string(), pre_check_item1.clone());

        let single_test_step1 = SingleTestStepStatus {
            step_id: "st_001".to_string(),
            command_from_control: Some("INITIATE_TEST".to_string()),
            params_from_control: None,
            execution_status_from_site: None,
            result_data_from_site: None,
            feedback_notes_from_site: None,
            confirmation_status_from_control: None,
            last_updated: Utc::now(),
        };
        original_state.single_test_steps.insert("st_001".to_string(), single_test_step1.clone());
        original_state.version = 1;


        let serialized = serde_json::to_string_pretty(&original_state).unwrap();
        // println!("Serialized TaskDebugState:\n{}", serialized); // 用于调试
        let deserialized: TaskDebugState = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original_state.task_id, deserialized.task_id);
        assert_eq!(original_state.last_updated_by_role, deserialized.last_updated_by_role);
        assert_eq!(original_state.version, deserialized.version);
        assert_eq!(original_state.pre_check_items.get("pc_001").unwrap().item_id, deserialized.pre_check_items.get("pc_001").unwrap().item_id);
        assert_eq!(original_state.single_test_steps.get("st_001").unwrap().step_id, deserialized.single_test_steps.get("st_001").unwrap().step_id);
        assert!((original_state.last_update_timestamp - deserialized.last_update_timestamp).num_milliseconds().abs() < 1000, "Timestamp mismatch too large for task debug state");


        // Test empty state
        let empty_state = TaskDebugState::new("task_empty_456".to_string());
        let serialized_empty = serde_json::to_string(&empty_state).unwrap();
        let deserialized_empty: TaskDebugState = serde_json::from_str(&serialized_empty).unwrap();
        assert_eq!(empty_state.task_id, deserialized_empty.task_id);
        assert!(deserialized_empty.pre_check_items.is_empty());
        assert!(deserialized_empty.single_test_steps.is_empty());
    }
} 