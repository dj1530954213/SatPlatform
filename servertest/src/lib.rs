//! `servertest` 服务端核心库。
//! 
//! 本 Crate 是 `SatCloudService` 原Tauri应用的后端部分独立实现，包含了应用的主要业务逻辑、
//! WebSocket 服务、API 接口、配置管理、状态管理等核心功能模块。
//! 
//! 主要模块包括：
//! - `api`: 定义和处理外部 HTTP API 请求。
//! - `config`: 管理应用的配置信息加载与访问。
//! - `db`: (规划中) 数据库交互逻辑。
//! - `error`: 定义应用特定的错误类型。
//! - `mq`: (规划中) 消息队列相关功能。
//! - `state`: 管理应用级别的共享状态。
//! - `ws_server`: 实现 WebSocket 服务端，处理客户端连接、消息路由和实时通信。

pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod mq;
pub mod state;
pub mod ws_server; 