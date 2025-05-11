// common_models/src/ws_payloads.rs

//! 包含 WebSocket 通信中使用的各种 Payload 结构体定义。

// 暂时为空，后续会根据开发步骤添加具体内容。

// 根据 P0.3.1 和项目规则 2.1 定义 EchoPayload
use serde::{Serialize, Deserialize};
use crate::enums::ClientRole; // 假设 ClientRole 在 common_models/src/enums.rs 中定义
use uuid::Uuid;

/// "Echo" 消息的消息类型常量。
pub const ECHO_MESSAGE_TYPE: &str = "Echo";
/// "ErrorResponse" 消息的消息类型常量。
pub const ERROR_RESPONSE_MESSAGE_TYPE: &str = "ErrorResponse";
/// "Ping" 消息的消息类型常量。
pub const PING_MESSAGE_TYPE: &str = "Ping";
/// "Pong" 消息的消息类型常量。
pub const PONG_MESSAGE_TYPE: &str = "Pong";
/// "Register" 消息的消息类型常量 (P3.1.1, P4.1.1)。
pub const REGISTER_MESSAGE_TYPE: &str = "Register";
/// 注册响应消息类型 - 由服务器回应客户端的注册请求。
pub const REGISTER_RESPONSE_MESSAGE_TYPE: &str = "RegisterResponse";
/// 伙伴状态更新消息类型 - 服务器通知组内一个客户端其伙伴的在线状态变化。
pub const PARTNER_STATUS_UPDATE_MESSAGE_TYPE: &str = "PartnerStatusUpdate";

// --- 任务调试相关消息类型 (P3.3.1) ---

/// 用于客户端（现场端或中心端）更新预检查项状态的消息类型。
pub const UPDATE_PRE_CHECK_ITEM_TYPE: &str = "UpdatePreCheckItem";

/// 用于中心端向现场端发起（指令执行）单体测试步骤的消息类型。
pub const START_SINGLE_TEST_STEP_TYPE: &str = "StartSingleTestStep";

/// 用于现场端向中心端反馈单体测试步骤执行结果的消息类型。
pub const FEEDBACK_SINGLE_TEST_STEP_TYPE: &str = "FeedbackSingleTestStep";

/// 用于中心端确认现场端反馈的单体测试步骤结果的消息类型。
pub const CONFIRM_SINGLE_TEST_STEP_TYPE: &str = "ConfirmSingleTestStep";

/// 用于云端主动向客户端推送完整的、已更新的任务调试状态的消息类型。
pub const TASK_STATE_UPDATE_MESSAGE_TYPE: &str = "TaskStateUpdate";

/// EchoPayload 是一个简单的负载，用于测试 WebSocket 通信。
/// 它包含一个字符串内容，期望被服务器回显。
///
/// 根据规则 2.1，所有共享模型都必须派生 `Serialize`, `Deserialize`, `Debug`, `Clone`。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EchoPayload {
    /// 需要回显的内容。
    pub content: String,
} 

/// ErrorResponsePayload 用于在处理请求发生错误时，向客户端回复标准的错误信息。
///
/// 根据规则 2.1，所有共享模型都必须派生 `Serialize`, `Deserialize`, `Debug`, `Clone`。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ErrorResponsePayload {
    /// 可选字段，指明原始请求的消息类型（如果可知）。
    /// 例如，如果处理 "RegisterClient" 消息时出错，这里可以是 "RegisterClient"。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_message_type: Option<String>,
    /// 错误的详细描述信息。
    pub error: String,
}

/// PingPayload 是客户端发送到服务端的心跳消息负载。
/// 当前为空结构体，但定义它有助于类型安全和未来的扩展。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PingPayload {}

/// PongPayload 是服务端响应客户端心跳（Ping）的消息负载。
/// 当前为空结构体。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PongPayload {}

/// 客户端注册时发送的负载。
///
/// 用于客户端向服务器声明其身份、希望加入或创建的组以及关联的任务。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterPayload {
    /// 客户端希望加入或创建的组的ID。
    /// 这个ID通常由用户（例如控制中心操作员）提供，或者基于任务信息生成。
    pub group_id: String,
    /// 客户端声明的角色 (例如，控制中心或现场移动端)。
    pub role: ClientRole,
    /// 客户端希望关联的调试任务的唯一ID。
    /// 此ID用于在云端初始化或关联到特定的任务状态。
    pub task_id: String,
}

/// 服务器对 "Register" 消息的响应负载。
///
/// 告知客户端注册/加入组操作的结果，并分配/确认客户端ID。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterResponsePayload {
    /// 指示注册/加入组操作是否成功。
    pub success: bool,
    /// 可选的附加信息，例如成功消息或失败原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 服务器为该客户端会话分配的唯一ID (`Uuid`)。
    /// 此ID在 `ClientSession` 创建时已生成，此处用于客户端接收和确认。
    pub assigned_client_id: Uuid,
    /// 如果注册成功，客户端实际加入或创建的组的ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_group_id: Option<String>,
    /// 如果注册成功，服务器为客户端最终确定的角色。
    /// 通常与客户端请求的角色一致，但服务器可能有最终决定权。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_role: Option<ClientRole>,
}

/// 伙伴状态更新负载。
///
/// 用于通知组内伙伴客户端其伙伴的上下线状态。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartnerStatusPayload {
    /// 发生状态变化的伙伴的角色 (例如，是控制中心上线了，还是现场移动端下线了)。
    pub partner_role: ClientRole,
    /// 发生状态变化的伙伴的客户端ID (`Uuid`)。
    pub partner_client_id: Uuid,
    /// 指示伙伴的在线状态。
    /// `true` 表示上线/加入组，`false` 表示下线/离开组。
    pub is_online: bool,
    /// 发生此状态变化的伙伴所属的组ID。
    pub group_id: String,
}

// P0.3.1_Test: EchoPayload 单元测试
#[cfg(test)]
mod tests {
    use super::*; // 导入 EchoPayload
    use serde_json; // 用于序列化和反序列化
    use crate::enums::ClientRole; // 为测试导入 ClientRole
    use uuid::Uuid;

    #[test]
    fn test_echo_payload_serialization_deserialization() {
        // 创建一个 EchoPayload 实例
        let original_payload = EchoPayload {
            content: "Hello, WebSocket!".to_string(),
        };

        // 1. 测试序列化
        let serialized_payload = serde_json::to_string(&original_payload);
        assert!(serialized_payload.is_ok(), "EchoPayload 序列化失败");
        let json_string = serialized_payload.unwrap();

        // 简单的验证，确保 content 字段存在且值正确
        // 对于更复杂的结构，可能需要更详细的 JSON 结构检查
        assert!(json_string.contains("content"));
        assert!(json_string.contains("Hello, WebSocket!"));

        // 2. 测试反序列化
        let deserialized_payload_result = serde_json::from_str::<EchoPayload>(&json_string);
        assert!(deserialized_payload_result.is_ok(), "EchoPayload 反序列化失败");
        let deserialized_payload = deserialized_payload_result.unwrap();

        // 3. 断言原始实例和反序列化后的实例相等
        assert_eq!(original_payload, deserialized_payload, "序列化和反序列化后的 EchoPayload 不相等");
    }

    #[test]
    fn test_error_response_payload_serialization_deserialization_with_type() {
        let original_payload = ErrorResponsePayload {
            original_message_type: Some("TestRequest".to_string()),
            error: "Something went wrong".to_string(),
        };

        let serialized_payload = serde_json::to_string(&original_payload);
        assert!(serialized_payload.is_ok(), "ErrorResponsePayload (with type) 序列化失败");
        let json_string = serialized_payload.unwrap();

        assert!(json_string.contains("original_message_type"));
        assert!(json_string.contains("TestRequest"));
        assert!(json_string.contains("error"));
        assert!(json_string.contains("Something went wrong"));

        let deserialized_payload_result = serde_json::from_str::<ErrorResponsePayload>(&json_string);
        assert!(deserialized_payload_result.is_ok(), "ErrorResponsePayload (with type) 反序列化失败");
        let deserialized_payload = deserialized_payload_result.unwrap();
        
        assert_eq!(original_payload, deserialized_payload, "序列化和反序列化后的 ErrorResponsePayload (with type) 不相等");
    }

    #[test]
    fn test_error_response_payload_serialization_deserialization_without_type() {
        let original_payload = ErrorResponsePayload {
            original_message_type: None,
            error: "Another issue".to_string(),
        };

        let serialized_payload = serde_json::to_string(&original_payload);
        assert!(serialized_payload.is_ok(), "ErrorResponsePayload (without type) 序列化失败");
        let json_string = serialized_payload.unwrap();
        
        // 当 Option 是 None 时，serde_json 默认不序列化该字段
        assert!(!json_string.contains("original_message_type")); 
        assert!(json_string.contains("error"));
        assert!(json_string.contains("Another issue"));

        let deserialized_payload_result = serde_json::from_str::<ErrorResponsePayload>(&json_string);
        assert!(deserialized_payload_result.is_ok(), "ErrorResponsePayload (without type) 反序列化失败");
        let deserialized_payload = deserialized_payload_result.unwrap();

        assert_eq!(original_payload, deserialized_payload, "序列化和反序列化后的 ErrorResponsePayload (without type) 不相等");
    }

    #[test]
    fn test_ping_payload_serialization_deserialization() {
        let original_payload = PingPayload {};
        let serialized_payload = serde_json::to_string(&original_payload).expect("PingPayload 序列化失败");
        // 空结构体序列化后应为 "{}"
        assert_eq!(serialized_payload, "{}");

        let deserialized_payload: PingPayload = serde_json::from_str(&serialized_payload).expect("PingPayload 反序列化失败");
        assert_eq!(original_payload, deserialized_payload);
    }

    #[test]
    fn test_pong_payload_serialization_deserialization() {
        let original_payload = PongPayload {};
        let serialized_payload = serde_json::to_string(&original_payload).expect("PongPayload 序列化失败");
        // 空结构体序列化后应为 "{}"
        assert_eq!(serialized_payload, "{}");

        let deserialized_payload: PongPayload = serde_json::from_str(&serialized_payload).expect("PongPayload 反序列化失败");
        assert_eq!(original_payload, deserialized_payload);
    }

    // 为 RegisterPayload 添加测试
    #[test]
    fn test_register_payload_serialization_deserialization() {
        let payload = RegisterPayload {
            group_id: "test_group_123".to_string(),
            role: ClientRole::ControlCenter,
            task_id: "task_abc_789".to_string(),
        };

        // 测试序列化
        let serialized = serde_json::to_string(&payload).expect("RegisterPayload serialization failed");
        
        // 预期JSON字符串 (字段顺序可能不同，但内容应匹配)
        // "{\"group_id\":\"test_group_123\",\"role\":\"ControlCenter\",\"task_id\":\"task_abc_789\"}"
        assert!(serialized.contains("\"group_id\":\"test_group_123\""));
        assert!(serialized.contains("\"role\":\"ControlCenter\""));
        assert!(serialized.contains("\"task_id\":\"task_abc_789\""));

        // 测试反序列化
        let deserialized: RegisterPayload = serde_json::from_str(&serialized).expect("RegisterPayload deserialization failed");
        assert_eq!(payload.group_id, deserialized.group_id);
        assert_eq!(payload.role, deserialized.role);
        assert_eq!(payload.task_id, deserialized.task_id);
    }

    #[test]
    fn test_register_payload_clone_debug() {
        let payload = RegisterPayload {
            group_id: "clone_group".to_string(),
            role: ClientRole::OnSiteMobile,
            task_id: "clone_task".to_string(),
        };
        let cloned_payload = payload.clone();
        assert_eq!(payload.group_id, cloned_payload.group_id);
        assert_eq!(payload.role, cloned_payload.role);
        assert_eq!(payload.task_id, cloned_payload.task_id);
        // 简单验证 Debug trait 是否产生输出
        assert!(!format!("{:?}", payload).is_empty());
    }

    // 为 RegisterResponsePayload 编写单元测试
    #[test]
    fn test_register_response_payload_serialization_deserialization_success() {
        let client_uuid = Uuid::new_v4();
        let payload = RegisterResponsePayload {
            success: true,
            message: Some("Successfully registered!".to_string()),
            assigned_client_id: client_uuid,
            effective_group_id: Some("effective_group".to_string()),
            effective_role: Some(ClientRole::ControlCenter),
        };

        let serialized = serde_json::to_string(&payload).expect("RegisterResponsePayload serialization failed");
        assert!(serialized.contains("\"success\":true"));
        assert!(serialized.contains("\"message\":\"Successfully registered!\""));
        assert!(serialized.contains(&format!("\"assigned_client_id\":\"{}\"", client_uuid)));
        assert!(serialized.contains("\"effective_group_id\":\"effective_group\""));
        assert!(serialized.contains("\"effective_role\":\"ControlCenter\""));

        let deserialized: RegisterResponsePayload = serde_json::from_str(&serialized).expect("RegisterResponsePayload deserialization failed");
        assert_eq!(payload.success, deserialized.success);
        assert_eq!(payload.message, deserialized.message);
        assert_eq!(payload.assigned_client_id, deserialized.assigned_client_id);
        assert_eq!(payload.effective_group_id, deserialized.effective_group_id);
        assert_eq!(payload.effective_role, deserialized.effective_role);
    }

    #[test]
    fn test_register_response_payload_serialization_deserialization_failure_with_none() {
        let client_uuid = Uuid::new_v4();
        let payload = RegisterResponsePayload {
            success: false,
            message: Some("Role conflict.".to_string()),
            assigned_client_id: client_uuid, // 即使失败，也返回分配的ID
            effective_group_id: None,
            effective_role: None,
        };

        let serialized = serde_json::to_string(&payload).expect("RegisterResponsePayload serialization failed");
        assert!(serialized.contains("\"success\":false"));
        assert!(serialized.contains("\"message\":\"Role conflict.\""));
        assert!(!serialized.contains("\"effective_group_id\":null")); // skip_serializing_if = "Option::is_none"
        assert!(!serialized.contains("effective_group_id\":")); // 确认字段完全不存在
        assert!(!serialized.contains("\"effective_role\":null"));
        assert!(!serialized.contains("effective_role\":"));
        
        let deserialized: RegisterResponsePayload = serde_json::from_str(&serialized).expect("RegisterResponsePayload deserialization failed");
        assert_eq!(payload.success, deserialized.success);
        assert_eq!(payload.message, deserialized.message);
        assert_eq!(payload.assigned_client_id, deserialized.assigned_client_id);
        assert_eq!(payload.effective_group_id, None);
        assert_eq!(payload.effective_role, None);
    }

    // 为 PartnerStatusPayload 编写单元测试
    #[test]
    fn test_partner_status_payload_serialization_deserialization() {
        let partner_uuid = Uuid::new_v4();
        let payload = PartnerStatusPayload {
            partner_role: ClientRole::OnSiteMobile,
            partner_client_id: partner_uuid,
            is_online: true,
            group_id: "group_status_xyz".to_string(),
        };

        let serialized = serde_json::to_string(&payload).expect("PartnerStatusPayload serialization failed");
        assert!(serialized.contains("\"partner_role\":\"OnSiteMobile\""));
        assert!(serialized.contains(&format!("\"partner_client_id\":\"{}\"", partner_uuid)));
        assert!(serialized.contains("\"is_online\":true"));
        assert!(serialized.contains("\"group_id\":\"group_status_xyz\""));

        let deserialized: PartnerStatusPayload = serde_json::from_str(&serialized).expect("PartnerStatusPayload deserialization failed");
        assert_eq!(payload.partner_role, deserialized.partner_role);
        assert_eq!(payload.partner_client_id, deserialized.partner_client_id);
        assert_eq!(payload.is_online, deserialized.is_online);
        assert_eq!(payload.group_id, deserialized.group_id);
    }
} 