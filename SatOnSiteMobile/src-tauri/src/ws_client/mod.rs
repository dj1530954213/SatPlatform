// SatOnSiteMobile/src-tauri/src/ws_client/mod.rs

//! WebSocket 客户端服务模块根。

// 声明 'service' 子模块，对应于 service.rs 文件
pub mod service;

// 导出 'service' 模块中的 WebSocketClientService，方便外部使用
pub use service::WebSocketClientService;

// (如果之前有错误的 services 模块声明，确保它被移除或注释掉)
// pub mod services; // 移除或注释掉这一行 