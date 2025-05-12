// SatOnSiteMobile/src-tauri/src/ws_client/mod.rs

//! WebSocket 客户端模块，封装了与云端 WebSocket 服务的交互逻辑。

pub mod services;

// 公开 WebSocketClientService 以便在 main.rs 中使用
pub use service::WebSocketClientService; 