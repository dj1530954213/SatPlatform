// rust_websocket_utils/src/client/mod.rs

//! WebSocket 客户端模块。
//!
//! 本模块 (`client`) 及其子模块（如 `transport`）共同构成了 `rust_websocket_utils` 库中
//! 用于实现 WebSocket 客户端功能的核心组件。
//!
//! 主要职责包括：
//! - **连接建立**: 提供连接到远程 WebSocket 服务器的机制。
//! - **消息传输**: 管理通过 WebSocket 连接发送和接收消息的逻辑。
//! - **生命周期管理**: 处理连接的打开、关闭以及可能发生的错误，并通过回调或事件通知应用层。
//! - **传输层抽象**: 封装底层 WebSocket 库（如 `tokio-tungstenite`）的细节，
//!   提供一个更简洁、更易于使用的API给上层应用。
//!
//! `transport` 子模块通常包含具体的传输层实现，例如 `TransportLayer` 结构体及其相关方法。

pub mod transport; // 公开 transport 子模块，其中包含主要的客户端传输层逻辑 