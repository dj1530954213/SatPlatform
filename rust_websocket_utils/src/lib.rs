//! `rust_websocket_utils` - Rust WebSocket 通信实用工具库 Crate。
//!
//! 本 crate 提供了一套用于简化 Rust 项目中 WebSocket 客户端和服务器端通信实现的实用功能。
//! 它的设计目标是提供一个相对轻量级的传输层抽象，使得开发者可以更专注于业务逻辑，
//! 而不是底层的 WebSocket 协议细节和连接管理。
//!
//! 主要特性与模块：
//! - **核心消息结构 (`message`模块)**: 定义了标准的 WebSocket 消息封装，例如 `WsMessage` 结构体，
//!   该结构体通常包含 `message_type` (字符串，用于区分业务类型) 和 `payload` (通常是 JSON 字符串，承载具体数据)，
//!   以及可选的 `message_id` 和 `timestamp` 以支持追踪和调试。参考 `common_models` 以获取实际应用中 payload 的定义方式。
//!
//! - **错误处理 (`error`模块)**: 定义了库专用的错误类型，如 `WsServerError` (服务器端错误) 和
//!   `WsClientError` (客户端错误)，以及可能的 `TransportError`，用于清晰地表示在 WebSocket 操作过程中可能发生的各种问题。
//!
//! - **服务器端传输层 (`server`模块)**: 提供了启动和管理 WebSocket 服务器的功能。
//!   这通常包括监听指定端口、接受新的客户端连接、为每个连接分发消息处理任务，
//!   以及通过回调机制（如 `on_open`, `on_message`, `on_close`, `on_error`）将连接生命周期事件和接收到的消息通知给应用层代码。
//!   此模块旨在处理底层的 `tokio-tungstenite` (或类似库) 的复杂性。
//!
//! - **客户端传输层 (`client`模块)**: 提供了连接到 WebSocket 服务器、发送和接收消息的功能。
//!   与服务器端类似，它也通常通过回调机制来处理连接事件和消息，简化了客户端的异步通信逻辑。
//!
//! 使用场景：
//! 此库特别适用于需要 Rust 后端（例如基于 Actix, Axum, 或 Tauri 的应用）通过 WebSocket 与前端或其他服务进行实时、双向通信的场景。
//! 它鼓励与像 `common_models` 这样的共享数据模型库结合使用，以确保跨服务/跨语言边界的消息结构一致性。
//!
//! 注意：本库主要关注传输层和消息封装，具体的业务消息处理逻辑、序列化/反序列化策略（推荐 `serde_json`）
//! 以及应用层状态管理等，仍由使用本库的应用程序负责。

pub mod client; // 包含 WebSocket 客户端连接和通信逻辑
pub mod error;  // 定义库中使用的各种错误类型
pub mod message;// 定义核心的 WebSocket 消息结构，如 WsMessage
pub mod server; // 包含 WebSocket 服务器监听、连接管理和消息分发逻辑

/// 一个简单的示例函数，用于演示 crate 的基本可用性和测试流程。
/// 在实际的 `rust_websocket_utils` 库中，此类通用工具函数可能较少，
/// 主要侧重于 WebSocket 相关的模块化功能。
///
/// # Arguments
/// * `left` - 左操作数 (u64 类型)。
/// * `right` - 右操作数 (u64 类型)。
///
/// # Returns
/// 返回两个参数的和 (u64 类型)。
pub fn add(left: u64, right: u64) -> u64 {
    left + right // 执行加法运算
}

// 单元测试模块
#[cfg(test)]
mod tests {
    use super::*; // 导入父模块（即本 crate 的根）的所有公共项，主要是 add 函数

    // 一个简单的测试用例，验证 `add` 函数是否按预期工作。
    #[test]
    fn it_works() {
        let result = add(2, 2); // 调用 add 函数
        assert_eq!(result, 4, "2 + 2 应该等于 4"); // 断言结果是否为 4，并提供失败时的提示信息
    }
}
