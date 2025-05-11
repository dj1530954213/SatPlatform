// rust_websocket_utils/src/message.rs

//! 定义 WebSocket 通信中使用的核心消息结构。
//!
//! 本模块主要包含 `WsMessage` 结构体的定义及其相关实现。
//! `WsMessage` 作为客户端与服务端之间所有 WebSocket 消息交换的标准格式，
//! 确保了通信的统一性和可扩展性。它遵循了项目规则中关于消息结构、
//! 序列化/反序列化以及必要元数据（如消息ID和时间戳）的要求。

// 根据 P0.3.1 和项目规则 4.1, 1.2 定义 WsMessage
use serde::{Serialize, Deserialize}; // 引入 serde 用于序列化和反序列化 (项目规则 1.2)
use uuid::Uuid; // 用于生成唯一的 message_id (项目规则 1.2)
use chrono::Utc; // 用于生成消息时间戳 (项目规则 1.2)
use serde_json; // 用于将 payload 序列化/反序列化为 JSON 字符串
use crate::error::WsError; // 引入本库定义的错误类型，用于处理序列化/反序列化失败等情况

/// `WsMessage` 代表在客户端与 WebSocket 服务器之间进行交换的标准消息结构。
///
/// 此结构体封装了消息的基本元数据以及实际的业务数据负载。
/// 设计上旨在提供一个通用且类型安全的消息传递机制。
///
/// # 字段
/// - `message_id`: 一个通过 UUID v4 生成的唯一字符串标识符，用于追踪和区分每一条消息。
/// - `message_type`: 一个字符串，用于指明此消息的业务类型 (例如, "Echo", "Register", "TaskUpdate")。
///   接收方会根据此类型来决定如何解释和处理 `payload` 字段。
/// - `payload`: 消息的实际数据负载，表示为一个 JSON 格式的字符串。其内部的具体数据结构由 `message_type` 决定。
///   发送方负责将具体的业务数据结构序列化为此 JSON 字符串，接收方则负责将其反序列化回相应的结构体。
/// - `timestamp`: 一个 `i64` 类型的值，表示消息创建时的 UTC 时间戳 (自 Unix 纪元以来的毫秒数)。
///
/// 此结构体遵循项目规则 4.1 (WebSocket 消息结构) 和 1.2 (相关依赖库如 `uuid`, `chrono`, `serde`)。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WsMessage {
    /// 为此消息实例生成的唯一标识符 (UUID v4 格式的字符串)。
    /// 用于在复杂的通信场景中追踪消息或进行调试。
    pub message_id: String,

    /// 描述消息业务类型的字符串, 例如 "Echo", "RegisterClient", "TaskUpdate" 等。
    /// 此字段是路由和处理消息的关键，它告诉接收方 `payload` 中包含的是哪种类型的数据。
    pub message_type: String,

    /// 消息的实际数据负载，以 JSON 字符串的形式存储。
    /// 这种设计允许传输任意复杂的、与 `message_type` 对应的业务数据结构。
    pub payload: String,

    /// 消息在创建时的时间戳，以自 Unix 纪元 (1970-01-01T00:00:00Z) 以来的毫秒数表示 (UTC 时间)。
    /// 可用于消息排序、超时检测或审计日志。
    pub timestamp: i64,
}

impl WsMessage {
    /// 创建一个新的 `WsMessage` 实例。
    ///
    /// 此构造函数会自动生成一个唯一的 `message_id` (UUID v4) 和当前的 UTC 时间戳。
    /// 提供的 `payload_data` 会被序列化为 JSON 字符串并存储在 `payload` 字段中。
    ///
    /// # Arguments
    ///
    /// * `message_type` - 一个 `String`，表示此消息的业务类型。
    /// * `payload_data` - 一个实现了 `serde::Serialize` trait 的数据结构引用。此数据将被序列化为 JSON 字符串。
    ///
    /// # Returns
    ///
    /// * `Result<WsMessage, WsError>` - 如果 `payload_data` 成功序列化为 JSON，则返回包含新 `WsMessage` 实例的 `Ok`。
    ///   如果序列化过程中发生错误，则返回包含 `WsError::SerializationError` 的 `Err`。
    pub fn new<T: Serialize>(message_type: String, payload_data: &T) -> Result<WsMessage, WsError> {
        // 将传入的 payload_data 序列化为 JSON 字符串
        let payload_str = serde_json::to_string(payload_data)
            .map_err(|e| WsError::SerializationError(format!("创建 WsMessage 时序列化载荷失败: {}", e)))?;
        Ok(WsMessage {
            message_id: Uuid::new_v4().to_string(), // 生成新的 UUID v4 作为消息 ID
            message_type, // 使用传入的消息类型
            payload: payload_str, // 使用序列化后的 JSON 字符串作为载荷
            timestamp: Utc::now().timestamp_millis(), // 获取当前 UTC 时间的毫秒级时间戳
        })
    }

    /// 将内部存储的 JSON 字符串载荷反序列化为指定的目标类型 `T`。
    ///
    /// 此方法提供了一种便捷的方式来从 `WsMessage` 的 `payload` 字段中提取并转换出具体的业务数据结构。
    ///
    /// # 类型参数
    ///
    /// * `T` - 期望反序列化成的目标类型。此类型必须实现 `serde::de::DeserializeOwned` trait，
    ///   这意味着它可以从任何实现了 `serde::Deserializer` 的数据格式（在此场景中是 JSON）中被拥有式地反序列化。
    ///
    /// # Returns
    ///
    /// * `Result<T, WsError>` - 如果 `payload` 字符串成功反序列化为类型 `T` 的实例，则返回包含该实例的 `Ok`。
    ///   如果反序列化过程中发生错误（例如，JSON 格式不正确，或 JSON 结构与类型 `T` 不匹配），
    ///   则返回包含 `WsError::DeserializationError` 的 `Err`。
    pub fn deserialize_payload<T: for<'de> Deserialize<'de>>(&self) -> Result<T, WsError> {
        // 尝试从 self.payload (JSON 字符串) 反序列化为目标类型 T
        serde_json::from_str(&self.payload)
            .map_err(|e| WsError::DeserializationError(format!("WsMessage 载荷反序列化为目标类型失败: {}, 原始载荷: '{}'", e, self.payload)))
    }
}

// P0.3.1_Test: WsMessage 单元测试
#[cfg(test)]
mod tests {
    use super::*; // 导入当前模块 (message) 的所有公共项，主要是 WsMessage
    use common_models::ws_payloads::EchoPayload; // 从 common_models 导入 EchoPayload 用于测试
    // use serde_json; // serde_json 已在父模块通过 use 导入，通常不需要在此再次导入

    // 定义一个用于测试的消息类型常量字符串
    const TEST_MESSAGE_TYPE: &str = "测试回显类型"; // 此常量用于单元测试中的消息类型字段

    #[test]
    /// 测试 `WsMessage::new` 构造函数是否能成功创建一个 `WsMessage` 实例，
    /// 并验证其基本字段（如 `message_type`, `message_id`, `timestamp`）是否按预期初始化，
    /// 以及其 `payload` 字段是否能被正确反序列化回原始的 `EchoPayload`。
    fn test_ws_message_new_creation_and_payload_integrity() {
        // 准备一个 EchoPayload 实例作为测试数据
        let echo_payload = EchoPayload {
            content: "用于WsMessage构造函数测试的内容".to_string(), // 中文测试内容
        };

        // 调用 WsMessage::new 来创建消息
        let ws_message_result = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &echo_payload);
        // 断言：WsMessage::new 调用应成功，不返回错误
        assert!(ws_message_result.is_ok(), "WsMessage::new 创建消息实例失败，错误: {:?}", ws_message_result.err());

        let ws_message = ws_message_result.unwrap(); // 获取创建成功的 WsMessage 实例
        // 断言：消息类型应与传入的 TEST_MESSAGE_TYPE 一致
        assert_eq!(ws_message.message_type, TEST_MESSAGE_TYPE, "消息类型与预期不符");
        // 断言：消息 ID (UUID) 不应为空字符串
        assert!(!ws_message.message_id.is_empty(), "消息 ID (message_id) 不应为空");
        // 断言：时间戳应为一个正数 (这是一个非常基础的检查，表明时间戳已生成)
        assert!(ws_message.timestamp > 0, "时间戳 (timestamp) 应为正数");

        // 验证 WsMessage 内部的 payload 字符串是否能被正确反序列化回原始的 EchoPayload 结构
        let deserialized_echo_payload_result = serde_json::from_str::<EchoPayload>(&ws_message.payload);
        // 断言：从 payload 字符串反序列化 EchoPayload 应成功
        assert!(deserialized_echo_payload_result.is_ok(), "WsMessage 内部的 payload 字符串反序列化为 EchoPayload 失败: {:?}", deserialized_echo_payload_result.err());
        // 断言：反序列化得到的 EchoPayload 实例应与原始的 echo_payload 相等
        assert_eq!(deserialized_echo_payload_result.unwrap(), echo_payload, "从 payload 反序列化得到的 EchoPayload 与原始实例不相等");
    }

    #[test]
    /// 测试 `WsMessage::deserialize_payload` 方法是否能将 `WsMessage` 实例内部的
    /// JSON 字符串载荷成功反序列化为指定的目标类型 (`EchoPayload`)。
    fn test_ws_message_deserialize_payload_method() {
        // 准备一个原始的 EchoPayload 实例
        let original_echo_payload = EchoPayload {
            content: "用于测试 deserialize_payload 方法的载荷内容".to_string(), // 中文测试内容
        };
        // 使用原始 EchoPayload 创建一个 WsMessage 实例
        let ws_message = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &original_echo_payload).expect("测试前置条件：创建 WsMessage 失败，这不应发生");

        // 调用 WsMessage 实例的 deserialize_payload 方法，尝试将其载荷反序列化为 EchoPayload 类型
        let deserialized_result: Result<EchoPayload, WsError> = ws_message.deserialize_payload();
        // 断言：deserialize_payload 方法的调用应成功，不返回错误
        assert!(deserialized_result.is_ok(), "调用 WsMessage::deserialize_payload 方法失败: {:?}", deserialized_result.err());
        // 断言：通过 deserialize_payload 方法反序列化得到的 EchoPayload 实例应与原始的 original_echo_payload 相等
        assert_eq!(deserialized_result.unwrap(), original_echo_payload, "通过 deserialize_payload 方法获取的 EchoPayload 与原始实例不相等");
    }

    #[test]
    /// 测试 `WsMessage` 实例的完整序列化 (到 JSON 字符串) 和反序列化 (从 JSON 字符串回来) 流程。
    /// 确保所有字段在序列化和反序列化后保持其原始值或预期状态，特别是 `payload` 能够被正确恢复。
    fn test_ws_message_full_serialization_then_deserialization_cycle() {
        // 准备一个原始的 EchoPayload 和基于它的 WsMessage
        let original_echo_payload = EchoPayload {
            content: "用于测试完整序列化/反序列化周期的内容".to_string(), // 中文测试内容
        };
        let original_ws_message = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &original_echo_payload).expect("测试前置条件：创建原始 WsMessage 失败");

        // 步骤 1: 将原始的 WsMessage 实例序列化为 JSON 字符串
        let serialized_ws_message_result = serde_json::to_string(&original_ws_message);
        assert!(serialized_ws_message_result.is_ok(), "将 WsMessage 序列化为 JSON 字符串失败: {:?}", serialized_ws_message_result.err());
        let json_string = serialized_ws_message_result.unwrap();

        // 步骤 2: 将上一步得到的 JSON 字符串反序列化回一个新的 WsMessage 实例
        let deserialized_ws_message_result = serde_json::from_str::<WsMessage>(&json_string);
        assert!(deserialized_ws_message_result.is_ok(), "从 JSON 字符串反序列化回 WsMessage 实例失败: {:?}", deserialized_ws_message_result.err());
        let deserialized_ws_message = deserialized_ws_message_result.unwrap();

        // 步骤 3: 验证反序列化后的 WsMessage 实例的各个字段是否与原始实例一致
        // message_type, message_id, 和 timestamp 应该是直接相等的
        assert_eq!(original_ws_message.message_type, deserialized_ws_message.message_type, "message_type 在序列化/反序列化周期后不一致");
        assert_eq!(original_ws_message.message_id, deserialized_ws_message.message_id, "message_id 在序列化/反序列化周期后不一致");
        assert_eq!(original_ws_message.timestamp, deserialized_ws_message.timestamp, "timestamp 在序列化/反序列化周期后不一致");

        // 步骤 4: 验证反序列化后的 WsMessage 实例中的 payload 字段，看它是否能被正确地反序列化回原始的 EchoPayload
        let payload_from_deserialized_result: Result<EchoPayload, WsError> = deserialized_ws_message.deserialize_payload();
        assert!(payload_from_deserialized_result.is_ok(), "从反序列化后的 WsMessage 实例中提取其 payload 失败: {:?}", payload_from_deserialized_result.err());
        assert_eq!(payload_from_deserialized_result.unwrap(), original_echo_payload, "从反序列化后的 WsMessage 中提取的 EchoPayload 与原始实例不相等");
    }

    #[test]
    /// 测试当尝试将 `WsMessage` 的 `payload` 反序列化为一个不匹配的类型时，
    /// `deserialize_payload` 方法是否能正确返回一个 `WsError::DeserializationError`。
    fn test_deserialize_payload_to_mismatched_type_error_handling() {
        // 定义一个与 EchoPayload 结构不同的简单载荷类型，用于测试类型不匹配的反序列化
        #[derive(Serialize, Deserialize, Debug, PartialEq)] // 为测试方便，添加 Debug 和 PartialEq
        struct AnotherDistinctPayload {
            some_value: i32, // 字段名和类型都与 EchoPayload 不同
        }
        // 创建一个包含 EchoPayload 的 WsMessage
        let echo_payload = EchoPayload {
            content: "用于测试反序列化类型不匹配错误的载荷".to_string(), // 中文测试内容
        };
        let message_with_echo_payload = WsMessage::new(TEST_MESSAGE_TYPE.to_string(), &echo_payload).expect("测试前置条件：创建包含 EchoPayload 的 WsMessage 失败");

        // 尝试将 EchoPayload (序列化后存储在 message_with_echo_payload.payload 中)
        // 反序列化为 AnotherDistinctPayload 类型，这应该会导致错误。
        let deserialization_attempt_result: Result<AnotherDistinctPayload, WsError> = message_with_echo_payload.deserialize_payload();
        // 断言：反序列化尝试应失败，返回 Err
        assert!(deserialization_attempt_result.is_err(), "尝试将 EchoPayload 反序列化为不匹配的 AnotherDistinctPayload 类型时，预期应失败但成功了");
        
        // 进一步验证返回的错误是否为预期的 WsError::DeserializationError 类型
        match deserialization_attempt_result.err().unwrap() { // 解包 Err 以检查其内部的 WsError
            WsError::DeserializationError(details) => {
                // 这是预期的错误类型。可以进一步检查 details 字符串是否包含有用的信息（可选）
                println!("捕获到预期的反序列化错误，详情: {}", details); // 打印错误详情以供调试时查看
            }
            unexpected_error => {
                // 如果返回了其他类型的 WsError，则测试失败
                panic!("预期的错误类型是 WsError::DeserializationError，但收到了: {:?}", unexpected_error);
            }
        }
    }
}

// 移除最后的占位注释，因为它不再需要
// // 暂时为空，后续会根据开发步骤添加 WsMessage 等结构体定义。 