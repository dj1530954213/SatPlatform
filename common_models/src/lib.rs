//! `common_models` 公共模型库 crate。
//!
//! 本 crate 集中定义了在 `SatPlatform` 项目各个 Rust 组件（如 `SatCloudService` 云端服务、
//! `SatControlCenter` 控制中心桌面应用的 Tauri 后端、`SatOnSiteMobile` 现场移动端桌面应用的 Tauri 后端）
//! 以及潜在的 Web 前端（通过 TypeScript 类型对应）之间共享的核心数据结构和枚举类型。
//!
//! 主要包含以下类型的模型：
//! - **项目详情 (`project_details`)**: 与项目相关的元数据和配置信息。
//! - **任务信息 (`task_info`)**: 包含任务的详细描述、状态、预检查项、测试步骤等。
//! - **WebSocket 消息负载 (`ws_payloads`)**: 用于客户端与服务端之间通过 WebSocket 通信时传输的各类消息的 Payload 结构体，
//!   例如注册、Echo、Ping/Pong、伙伴状态更新、任务状态更新等。
//! - **通用枚举 (`enums`)**: 定义了项目中广泛使用的枚举类型，如客户端角色 (`ClientRole`)、任务状态等，以保证类型安全和一致性。
//!
//! 设计原则：
//! - **共享性**: 所有在此 crate 中定义的模型都旨在被多个其他 crate 共享使用。
//! - **序列化/反序列化**: 所有模型（结构体和枚举）都必须派生 `serde::Serialize` 和 `serde::Deserialize` traits，
//!   以便能够轻松地在不同格式（如 JSON）之间进行转换，这对于网络通信和持久化至关重要。
//! - **可调试性与克隆**: 所有模型也必须派生 `Debug` 和 `Clone` traits，以方便调试输出和创建副本。
//! - **一致性**: 通过统一管理这些共享模型，确保了跨不同语言边界（Rust <-> TypeScript）和不同服务边界时数据结构的一致性。

// 声明并公开项目中的各个模块
pub mod project_details;    // 与项目配置和元数据相关的模型
pub mod task_info;          // 与调试任务详细信息（状态、步骤等）相关的模型
pub mod ws_payloads;        // WebSocket 通信中使用的各种消息负载结构体
pub mod enums;              // 项目中通用的枚举类型定义
pub mod task_models;        // 新增：与调试任务具体状态和业务交互相关的模型 (P3.3.1)
pub mod templates;          // 新增 templates 模块声明

/// 一个简单的示例函数，用于演示 crate 的基本功能和测试。
/// 在实际的 `common_models` 库中，此类通用工具函数可能较少，主要侧重于数据结构定义。
///
/// # Arguments
/// * `left` - 左操作数 (u64类型)。
/// * `right` - 右操作数 (u64类型)。
///
/// # Returns
/// 返回两个参数的和 (u64类型)。
pub fn add(left: u64, right: u64) -> u64 {
    left + right // 执行加法操作
}

// 单元测试模块
#[cfg(test)]
mod tests {
    use super::*; // 导入父模块（即本 crate 的根）的所有公共项

    // 一个简单的测试用例，验证 `add` 函数是否按预期工作。
    #[test]
    fn it_works() {
        let result = add(2, 2); // 调用 add 函数
        assert_eq!(result, 4);   // 断言结果是否等于 4
    }
}

// 重新导出模块中的主要结构体，方便外部 crate 直接使用
// 例如： use common_models::ProjectDetails;
// 而不是 use common_models::project_details::ProjectDetails;
// pub use project_details::*; // 移除未使用的导入
// pub use task_info::*;       // 移除未使用的导入
// pub use ws_payloads::*; 
pub use task_models::*; // 新增：导出 task_models 中的所有公共项 (P3.3.1)

// 重新导出关键的结构体和枚举，使其更易于访问
// 例如 `use common_models::ClientRole;` 而不是 `use common_models::enums::ClientRole;`
pub use enums::ClientRole;
pub use task_models::{TaskDebugState, PreCheckItemStatus, SingleTestStepStatus};

use serde::{Deserialize, Serialize};
// Uuid 用于生成 ID，但通常我们将其作为 String 类型存储在结构体中，以简化序列化和跨语言互操作性。
// 如果确实需要在结构体中存储 Uuid 类型，则需要确保 serde 支持已启用，并正确处理。
// use uuid::Uuid;

/// 通用的 WebSocket 消息结构。
/// 所有通过 WebSocket 传输的消息都应遵循此格式。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WsMessage {
    /// 消息的唯一标识符，通常是一个 UUID v4 字符串。
    pub message_id: String,
    /// 消息发送时的时间戳 (Unix epoch milliseconds)。
    pub timestamp: i64,
    /// 消息类型，用于指示 payload 的具体结构和含义。
    /// 例如："Register", "TaskStateUpdate", "UpdatePreCheckItem"。
    pub message_type: String,
    /// 消息的实际内容，通常是一个序列化后的 JSON 字符串，
    /// 其具体结构由 `message_type` 决定。
    pub payload: String,
}

#[cfg(test)]
mod lib_tests { // 重命名测试模块以避免与子模块中的 `tests` 冲突，或者确保只有一个 `tests` 模块在此文件
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    // 确保 ws_payloads 模块在此作用域内可用，如果 WsMessage 的测试依赖它
    // 如果 ws_payloads 本身有自己的 tests 模块，这里的引用可能需要调整
    // 但 RegisterPayload 等类型应该通过 `use super::*` 或 `use crate::ws_payloads::*` 可用

    #[test]
    fn test_ws_message_serialization_deserialization() {
        let example_payload_struct = crate::ws_payloads::RegisterPayload { // 使用 crate:: 明确路径
            group_id: "test_group".to_string(),
            role: ClientRole::ControlCenter, // ClientRole 应通过 pub use enums::ClientRole; 可用
            task_id: "test_task".to_string(),
            client_software_version: None, // 添加 None 值
            client_display_name: None,   // 添加 None 值
        };
        let payload_str = serde_json::to_string(&example_payload_struct).unwrap();

        let original_ws_message = WsMessage {
            message_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().timestamp_millis(),
            message_type: crate::ws_payloads::REGISTER_MESSAGE_TYPE.to_string(), // 使用 crate:: 明确路径
            payload: payload_str,
        };

        let serialized = serde_json::to_string_pretty(&original_ws_message).unwrap();
        // println!("Serialized WsMessage:\n{}", serialized); // 用于调试
        let deserialized: WsMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original_ws_message.message_id, deserialized.message_id);
        assert_eq!(original_ws_message.timestamp, deserialized.timestamp);
        assert_eq!(original_ws_message.message_type, deserialized.message_type);
        assert_eq!(original_ws_message.payload, deserialized.payload);

        let deserialized_payload: crate::ws_payloads::RegisterPayload = // 使用 crate:: 明确路径
            serde_json::from_str(&deserialized.payload).unwrap();
        assert_eq!(example_payload_struct.group_id, deserialized_payload.group_id);
        assert_eq!(example_payload_struct.role, deserialized_payload.role);
    }
}

// 从 ws_payloads 中显式导出常量和 Payload 结构体
pub use crate::ws_payloads::{ 
    EchoPayload,
    ErrorResponsePayload,
    PingPayload,
    PongPayload,
    RegisterPayload,
    RegisterResponsePayload,
    PartnerStatusPayload,
    ECHO_MESSAGE_TYPE,
    ERROR_RESPONSE_MESSAGE_TYPE,
    PING_MESSAGE_TYPE,
    PONG_MESSAGE_TYPE,
    REGISTER_MESSAGE_TYPE,
    REGISTER_RESPONSE_MESSAGE_TYPE,
    PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
    UPDATE_PRE_CHECK_ITEM_TYPE,
    START_SINGLE_TEST_STEP_TYPE,
    FEEDBACK_SINGLE_TEST_STEP_TYPE,
    CONFIRM_SINGLE_TEST_STEP_TYPE,
    TASK_STATE_UPDATE_MESSAGE_TYPE,
};

// 从 task_models 模块显式导出业务相关的 Payload (如果它们确实在那里定义)
pub use crate::task_models::{UpdatePreCheckItemPayload, StartSingleTestStepPayload}; // 根据 Linter 修正路径
