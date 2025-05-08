// rust_websocket_utils/src/message.rs

//! 定义 WebSocket 消息结构。

// 根据 P0.3.1 和项目规则 4.1, 1.2 定义 WsMessage
use serde::{Serialize, Deserialize};
use uuid::Uuid; // 用于 message_id, 规则 1.2
use chrono::Utc; // 用于 timestamp, 规则 1.2

/// WsMessage 是所有客户端与服务端之间交换的 WebSocket 消息的标准结构。
///
/// 它包含以下字段：
/// - `message_id`: 使用 UUID v4 生成的唯一消息标识符。
/// - `message_type`: 消息类型字符串 (例如, "Echo", "Register", "TaskUpdate")，用于决定如何解析 `payload`。
/// - `payload`: 消息的实际数据负载，序列化为 JSON 字符串。其具体结构由 `message_type` 决定。
/// - `timestamp`: 消息创建时的时间戳 (Unix纪元以来的毫秒数, UTC)。
///
/// 此结构遵循项目规则 4.1。
/// 相关的 `uuid` 和 `chrono` 依赖遵循项目规则 1.2。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WsMessage {
    /// 为此消息生成的唯一标识符 (UUID v4)。
    pub message_id: String,

    /// 消息的类型, 例如 "Echo", "RegisterClient", "TaskUpdate"。
    /// 此字符串决定了 `payload` 应该如何被反序列化和处理。
    pub message_type: String,

    /// 消息的实际数据负载，序列化为 JSON 字符串。
    /// 此负载的具体结构由 `message_type` 决定。
    pub payload: String,

    /// 消息创建时的时间戳 (自 Unix 纪元以来的毫秒数, UTC)。
    pub timestamp: i64,
}

impl WsMessage {
    /// 使用给定的消息类型和可序列化的负载结构创建一个新的 `WsMessage`。
    ///
    /// `payload_struct` 参数应为实现了 `serde::Serialize` trait 的结构体。
    /// 此方法会自动将其序列化为 JSON 字符串，并生成 `message_id` 和 `timestamp`。
    ///
    /// # Arguments
    /// * `message_type`: 标识负载类型的字符串。
    /// * `payload_struct`: 要包含在此消息中的实际数据结构。
    ///
    /// # Errors
    /// 如果 `payload_struct` 序列化为 JSON 失败，则返回 `serde_json::Error`。
    pub fn new<T: Serialize>(message_type: &str, payload_struct: &T) -> Result<Self, serde_json::Error> {
        let payload_json = serde_json::to_string(payload_struct)?;
        Ok(WsMessage {
            message_id: Uuid::new_v4().to_string(),
            message_type: message_type.to_string(),
            payload: payload_json,
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    /// 将内部的 JSON 字符串 `payload` 反序列化为指定的类型 `T`。
    ///
    /// 类型 `T` 必须实现 `serde::Deserialize<'s>` trait, 其中 `'s` 是此 `WsMessage` 实例借用的生命周期。
    /// 这允许 `T` 借用 `payload` 中的数据。
    ///
    /// # Arguments
    /// * `self`: 对 `WsMessage` 实例的借用，其生命周期为 `'s`。
    ///
    /// # Errors
    /// 如果 `payload` 字符串无法被成功反序列化为类型 `T`，则返回 `serde_json::Error`。
    ///
    /// # Lifetimes
    /// * `'s`: `WsMessage` 实例的借用生命周期。反序列化后的类型 `T` 可能包含对此生命周期的引用，
    ///   确保了借用数据的安全性。
    pub fn deserialize_payload<'s, T>(&'s self) -> Result<T, serde_json::Error>
        where T: Deserialize<'s>
    {
        // &self.payload 的生命周期是 's
        // T 需要 Deserialize<'s>，这意味着它可以从生命周期为 's 的数据反序列化
        // serde_json::from_str 会正确处理这个，允许 T 借用 self.payload 中的数据
        serde_json::from_str(&self.payload)
    }
}

// P0.3.1_Test: WsMessage 单元测试
#[cfg(test)]
mod tests {
    use super::*; // 导入 WsMessage
    use common_models::ws_payloads::EchoPayload; // 导入 EchoPayload，需要 common_models 在依赖中
    use serde_json; // 用于序列化和反序列化

    const TEST_MESSAGE_TYPE: &str = "TestEcho";

    #[test]
    fn test_ws_message_new() {
        let echo_payload = EchoPayload {
            content: "Test content for WsMessage".to_string(),
        };

        let ws_message_result = WsMessage::new(TEST_MESSAGE_TYPE, &echo_payload);
        assert!(ws_message_result.is_ok(), "WsMessage::new 创建失败");

        let ws_message = ws_message_result.unwrap();
        assert_eq!(ws_message.message_type, TEST_MESSAGE_TYPE);
        assert!(!ws_message.message_id.is_empty(), "Message ID 不应为空");
        // 简单验证 timestamp 是否大于0 (一个非常基础的检查)
        assert!(ws_message.timestamp > 0, "Timestamp 应为正数");

        // 验证 payload 是否能被正确反序列化回 EchoPayload
        let deserialized_echo_payload_result = serde_json::from_str::<EchoPayload>(&ws_message.payload);
        assert!(deserialized_echo_payload_result.is_ok(), "WsMessage payload 反序列化为 EchoPayload 失败");
        assert_eq!(deserialized_echo_payload_result.unwrap(), echo_payload, "反序列化后的 EchoPayload 与原始的不相等");
    }

    #[test]
    fn test_ws_message_deserialize_payload() {
        let original_echo_payload = EchoPayload {
            content: "Payload for deserialization test".to_string(),
        };
        let ws_message = WsMessage::new("DeserializeTest", &original_echo_payload).unwrap();

        // 调用 deserialize_payload 方法
        let deserialized_result: Result<EchoPayload, _> = ws_message.deserialize_payload();
        assert!(deserialized_result.is_ok(), "deserialize_payload 方法失败");
        assert_eq!(deserialized_result.unwrap(), original_echo_payload, "通过 deserialize_payload 获取的 EchoPayload 与原始的不相等");
    }

    #[test]
    fn test_ws_message_full_serialization_deserialization() {
        let original_echo_payload = EchoPayload {
            content: "Full cycle test".to_string(),
        };
        let original_ws_message = WsMessage::new("FullCycleEcho", &original_echo_payload).unwrap();

        // 1. 序列化 WsMessage
        let serialized_ws_message = serde_json::to_string(&original_ws_message);
        assert!(serialized_ws_message.is_ok(), "WsMessage 序列化失败");
        let json_string = serialized_ws_message.unwrap();

        // 2. 反序列化 WsMessage
        let deserialized_ws_message_result = serde_json::from_str::<WsMessage>(&json_string);
        assert!(deserialized_ws_message_result.is_ok(), "WsMessage 反序列化失败");
        let deserialized_ws_message = deserialized_ws_message_result.unwrap();

        // 3. 验证字段 (message_type 应该相同)
        assert_eq!(original_ws_message.message_type, deserialized_ws_message.message_type);
        // message_id 和 timestamp 是动态生成的，但它们应该存在且格式大致正确
        // 这里我们主要关心的是 payload 是否能被正确恢复
        assert_eq!(original_ws_message.message_id, deserialized_ws_message.message_id, "Message ID 在序列化/反序列化后不一致");
        assert_eq!(original_ws_message.timestamp, deserialized_ws_message.timestamp, "Timestamp 在序列化/反序列化后不一致");

        // 4. 验证反序列化后的 WsMessage 中的 payload
        let payload_from_deserialized: Result<EchoPayload, _> = deserialized_ws_message.deserialize_payload();
        assert!(payload_from_deserialized.is_ok(), "从反序列化后的 WsMessage 中提取 payload 失败");
        assert_eq!(payload_from_deserialized.unwrap(), original_echo_payload, "从反序列化后的 WsMessage 中提取的 EchoPayload 与原始的不相等");
    }
}

// 暂时为空，后续会根据开发步骤添加 WsMessage 等结构体定义。 