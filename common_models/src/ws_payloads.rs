// common_models/src/ws_payloads.rs

//! 包含 WebSocket 通信中使用的各种 Payload 结构体定义。

// 暂时为空，后续会根据开发步骤添加具体内容。

// 根据 P0.3.1 和项目规则 2.1 定义 EchoPayload
use serde::{Serialize, Deserialize};

/// "Echo" 消息的消息类型常量。
pub const ECHO_MESSAGE_TYPE: &str = "Echo";
/// "ErrorResponse" 消息的消息类型常量。
pub const ERROR_RESPONSE_MESSAGE_TYPE: &str = "ErrorResponse";
/// "Ping" 消息的消息类型常量。
pub const PING_MESSAGE_TYPE: &str = "Ping";
/// "Pong" 消息的消息类型常量。
pub const PONG_MESSAGE_TYPE: &str = "Pong";

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

// P0.3.1_Test: EchoPayload 单元测试
#[cfg(test)]
mod tests {
    use super::*; // 导入 EchoPayload
    use serde_json; // 用于序列化和反序列化

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
} 