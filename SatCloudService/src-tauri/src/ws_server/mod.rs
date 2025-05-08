// SatCloudService/src-tauri/src/ws_server/mod.rs

//! WebSocket 服务端逻辑模块。

pub mod service;
pub mod connection_manager; // P1.2.1
pub mod message_router;     // P1.3.1
pub mod heartbeat_monitor;  // P1.4.1
// pub mod data_synchronizer; // P3.3.1 (稍后添加) 