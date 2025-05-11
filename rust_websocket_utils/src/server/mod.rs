// rust_websocket_utils/src/server/mod.rs

//! WebSocket 服务端模块。
//!
//! 本模块 (`server`) 及其子模块（例如 `transport`）共同负责提供 `rust_websocket_utils` 库中
//! 与 WebSocket 服务器端功能相关的组件和逻辑。
//!
//! 主要职责包括：
//! - **服务器启动与监听**: 提供在指定网络地址和端口上启动 WebSocket 服务器并开始监听传入连接的机制。
//! - **连接管理**: 处理新的客户端连接请求，包括 WebSocket 握手过程。
//! - **消息处理与分发**: 为每个成功建立的连接创建独立的处理流程，负责接收来自客户端的消息，
//!   并将这些消息（或连接事件）通过回调机制传递给上层应用逻辑进行处理。
//!   同时，也提供向上层应用暴露发送消息到特定客户端的能力。
//! - **传输层抽象**: 封装底层 WebSocket 库（如 `tokio-tungstenite`）的实现细节，
//!   旨在为开发者提供一个更简洁、事件驱动的 API 来构建 WebSocket 服务端应用。
//!
//! `transport` 子模块通常包含具体的传输层实现，例如 `start_server` 函数和 `ConnectionHandler` 结构体等。

pub mod transport; // 公开 transport 子模块，其中包含了主要的服务器端传输层逻辑和核心功能实现 