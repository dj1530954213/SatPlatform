// common_models/src/ws_payloads.rs

//! 包含 WebSocket 通信中使用的各种 Payload 结构体定义。

// 暂时为空，后续会根据开发步骤添加具体内容。

// 根据 P0.3.1 和项目规则 2.1 定义 EchoPayload
use serde::{Serialize, Deserialize};

/// EchoPayload 是一个简单的负载，用于测试 WebSocket 通信。
/// 它包含一个字符串内容，期望被服务器回显。
///
/// 根据规则 2.1，所有共享模型都必须派生 `Serialize`, `Deserialize`, `Debug`, `Clone`。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EchoPayload {
    /// 需要回显的内容。
    pub content: String,
} 

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
} 