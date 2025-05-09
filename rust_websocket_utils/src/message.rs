// rust_websocket_utils/src/message.rs

//! 定义 WebSocket 消息结构。

// 根据 P0.3.1 和项目规则 4.1, 1.2 定义 WsMessage
use serde::{Serialize, Deserialize};
use uuid::Uuid; // 用于 message_id, 规则 1.2
use chrono::Utc; // 用于 timestamp, 规则 1.2
use serde_json;
use crate::error::WsError;

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
    /// 创建一个新的 WsMessage 实例
    ///
    /// # Arguments
    ///
    /// * `message_type` - 消息类型字符串
    /// * `payload_data` - 任何实现了 `serde::Serialize` 的数据结构，将被序列化为 JSON 字符串
    ///
    /// # Returns
    ///
    /// 返回一个 `Result<WsMessage, WsError>`，如果载荷序列化成功则包含 `WsMessage`，否则包含错误。
    pub fn new<T: Serialize>(message_type: String, payload_data: &T) -> Result<WsMessage, WsError> {
        let payload_str = serde_json::to_string(payload_data)
            .map_err(|e| WsError::SerializationError(e.to_string()))?;
        Ok(WsMessage {
            message_id: Uuid::new_v4().to_string(),
            message_type,
            payload: payload_str,
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    /// 将内部的 JSON 字符串载荷反序列化为指定的类型
    ///
    /// # Type Parameters
    ///
    /// * `T` - 目标类型，必须实现 `serde::de::DeserializeOwned`
    ///
    /// # Returns
    ///
    /// 返回一个 `Result<T, WsError>`，如果反序列化成功则包含目标类型的实例，否则包含错误。
    pub fn deserialize_payload<T: for<'de> Deserialize<'de>>(&self) -> Result<T, WsError> {
        serde_json::from_str(&self.payload)
            .map_err(|e| WsError::DeserializationError(e.to_string()))
    }
}

// P0.3.1_Test: WsMessage 单元测试
#[cfg(test)]
mod tests {
    use super::*; // 导入 WsMessage
    use common_models::ws_payloads::EchoPayload; // 导入 EchoPayload，需要 common_models 在依赖中
    // use serde_json; // serde_json 已在父模块导入，这里通常不需要再次导入，除非有特殊用法

    const TEST_MESSAGE_TYPE: &str = "测试回显类型"; // 中文化常量内容

    #[test]
    fn test_ws_message_new() {
        let echo_payload = EchoPayload {
            content: "用于WsMessage的测试内容".to_string(),
        };

        let ws_message_result = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &echo_payload);
        assert!(ws_message_result.is_ok(), "WsMessage::new 创建失败");

        let ws_message = ws_message_result.unwrap();
        assert_eq!(ws_message.message_type, TEST_MESSAGE_TYPE);
        assert!(!ws_message.message_id.is_empty(), "消息ID不应为空");
        // 简单验证 timestamp 是否大于0 (一个非常基础的检查)
        assert!(ws_message.timestamp > 0, "时间戳应为正数");

        // 验证 payload 是否能被正确反序列化回 EchoPayload
        let deserialized_echo_payload_result = serde_json::from_str::<EchoPayload>(&ws_message.payload);
        assert!(deserialized_echo_payload_result.is_ok(), "WsMessage载荷反序列化为EchoPayload失败");
        assert_eq!(deserialized_echo_payload_result.unwrap(), echo_payload, "反序列化后的EchoPayload与原始的不相等");
    }

    #[test]
    fn test_ws_message_deserialize_payload() {
        let original_echo_payload = EchoPayload {
            content: "用于反序列化测试的载荷".to_string(),
        };
        let ws_message = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &original_echo_payload).unwrap();

        // 调用 deserialize_payload 方法
        let deserialized_result: Result<EchoPayload, _> = ws_message.deserialize_payload();
        assert!(deserialized_result.is_ok(), "deserialize_payload 方法失败");
        assert_eq!(deserialized_result.unwrap(), original_echo_payload, "通过deserialize_payload获取的EchoPayload与原始的不相等");
    }

    #[test]
    fn test_ws_message_full_serialization_deserialization() {
        let original_echo_payload = EchoPayload {
            content: "完整周期测试".to_string(),
        };
        let original_ws_message = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &original_echo_payload).unwrap();

        // 1. 序列化 WsMessage
        let serialized_ws_message = serde_json::to_string(&original_ws_message);
        assert!(serialized_ws_message.is_ok(), "WsMessage序列化失败");
        let json_string = serialized_ws_message.unwrap();

        // 2. 反序列化 WsMessage
        let deserialized_ws_message_result = serde_json::from_str::<WsMessage>(&json_string);
        assert!(deserialized_ws_message_result.is_ok(), "WsMessage反序列化失败");
        let deserialized_ws_message = deserialized_ws_message_result.unwrap();

        // 3. 验证字段 (message_type 应该相同)
        assert_eq!(original_ws_message.message_type, deserialized_ws_message.message_type);
        // message_id 和 timestamp 是动态生成的，但它们应该存在且格式大致正确
        // 这里我们主要关心的是 payload 是否能被正确恢复
        assert_eq!(original_ws_message.message_id, deserialized_ws_message.message_id, "消息ID在序列化/反序列化后不一致");
        assert_eq!(original_ws_message.timestamp, deserialized_ws_message.timestamp, "时间戳在序列化/反序列化后不一致");

        // 4. 验证反序列化后的 WsMessage 中的 payload
        let payload_from_deserialized: Result<EchoPayload, _> = deserialized_ws_message.deserialize_payload();
        assert!(payload_from_deserialized.is_ok(), "从反序列化后的WsMessage中提取payload失败");
        assert_eq!(payload_from_deserialized.unwrap(), original_echo_payload, "从反序列化后的WsMessage中提取的EchoPayload与原始的不相等");
    }

    #[test]
    fn test_deserialize_payload_error() {
        #[derive(Serialize, Deserialize)]
        struct AnotherPayload {
            value: i32,
        }
        let echo_payload = EchoPayload {
            content: "测试".to_string(),
        };
        let message = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &echo_payload).unwrap();

        // 尝试将 EchoPayload 反序列化为 AnotherPayload，应该失败
        let result: Result<AnotherPayload, _> = message.deserialize_payload();
        assert!(result.is_err());
        if let Err(WsError::DeserializationError(_)) = result {
            // 这是预期的错误类型
        } else {
            panic!("预期的错误类型是 DeserializationError");
        }
    }
}

// 移除最后的占位注释，因为它不再需要
// // 暂时为空，后续会根据开发步骤添加 WsMessage 等结构体定义。 