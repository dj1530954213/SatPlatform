// SatOnSiteMobile/src-tauri/src/ws_client/mod.rs

//! `SatOnSiteMobile` (现场端移动应用) 的 WebSocket 客户端模块根文件。
//!
//! 本模块 (`ws_client`) 封装了与云端 `SatCloudService` 进行 WebSocket 通信的所有核心逻辑。
//! 主要包括连接管理、消息收发、心跳维持、状态同步以及相关的事件处理。
//!
//! ## 核心组件：
//! - `service.rs` (`pub mod service`): 包含核心的 `WebSocketClientService` 结构体及其实现，
//!   该服务是 WebSocket 通信的主要协调者。
//!
//! ## 使用方式：
//! 通过 `pub use service::WebSocketClientService;` 将 `WebSocketClientService` 导出，
//! 使得应用的其他部分（例如 `main.rs` 中的 `setup` 钩子）可以方便地创建和管理此服务的实例。

// --- 子模块声明 --- 

/// 定义核心的 WebSocket 客户端服务 (`WebSocketClientService`)。
/// `service.rs` 文件包含了该服务的具体实现。
pub mod service;

// --- 公开导出 (Re-export) --- 

/// 从 `service` 子模块中公开导出 `WebSocketClientService` 结构体。
/// 这使得其他模块可以直接通过 `crate::ws_client::WebSocketClientService` 的路径来引用它，
/// 而无需写成 `crate::ws_client::service::WebSocketClientService`。
pub use service::WebSocketClientService;

// 注意：确保没有冗余或错误的模块声明。
// 例如，如果之前有一个名为 `services` 的模块但已被 `service` 取代，
// 应确保旧的声明 (如 `pub mod services;`) 已被移除或正确注释掉，以避免编译错误。
// // pub mod services; // 示例：如果存在错误的 services 模块声明，应移除或注释掉。 