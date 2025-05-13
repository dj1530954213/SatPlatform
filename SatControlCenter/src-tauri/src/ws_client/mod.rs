// SatControlCenter/src-tauri/src/ws_client/mod.rs

//! `SatControlCenter` (卫星控制中心) 的 WebSocket 客户端核心逻辑模块。
//!
//! 本模块 (`ws_client`) 及其子模块 (特别是 `service.rs`) 封装了所有与云端服务
//! (`SatCloudService`) 进行实时、双向 WebSocket 通信所需的功能。这是控制中心与云平台
//! 进行数据交换、命令传递和状态同步的主要通道。
//!
//! # 主要职责
//! - **连接管理**: 负责建立、维护、监控和在需要时自动重连到云端 WebSocket 服务。
//! - **消息收发**: 
//!   - 发送由控制中心生成的指令、数据或状态更新到云端 (使用 `WsMessage` 结构)。
//!   - 接收来自云端的消息 (同样使用 `WsMessage` 结构)，并将其路由到合适的处理器或通过 Tauri 事件通知前端。
//! - **心跳机制**: 实现心跳包的定时发送与响应检查，以维持连接活跃并检测死连接。
//! - **认证与会话管理**: 处理与云端服务的连接认证 (如果需要)，并管理会话状态。
//! - **状态同步与事件通知**: 将 WebSocket 连接状态的变化 (连接成功、失败、断开等) 以及收到的重要消息，
//!   通过 Tauri 的事件系统 (`app_handle.emit_all`) 通知给前端 Angular 应用。
//! - **错误处理**: 对连接错误、消息序列化/反序列化错误、超时等问题进行健壮的处理和日志记录。
//!
//! # 核心组件
//! - `service::WebSocketClientService`: 此结构体是本模块的核心，包含了大部分实际的 WebSocket 操作逻辑。
//!   它被设计为可在 Tauri 应用中作为托管状态 (`tauri::State`) 进行共享和访问。

/// `service` 子模块，包含了 `WebSocketClientService` 的主要实现。
/// 这个服务负责处理 WebSocket 连接的建立、消息的收发、心跳维持等核心功能。
pub mod service;

/// 从 `service` 子模块中公开导出 `WebSocketClientService` 结构体。
/// 这样做使得其他模块 (例如 `main.rs` 或 `commands` 模块) 可以直接通过
/// `crate::ws_client::WebSocketClientService` 路径来引用此核心服务结构体，
/// 而无需关心其在 `ws_client` 模块内部的具体文件组织结构。
pub use service::WebSocketClientService; 