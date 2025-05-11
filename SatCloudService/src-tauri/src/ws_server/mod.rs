// SatCloudService/src-tauri/src/ws_server/mod.rs

//! WebSocket 服务端的核心逻辑模块。
//!
//! 本模块是 `SatCloudService` 中负责处理 WebSocket 通信的中心枢纽。
//! 它包含了所有与 WebSocket 服务器运行、客户端交互和数据同步相关的功能组件。
//!
//! 主要子模块包括：
//! - `service`:             负责启动和管理底层的 WebSocket 服务监听，接受新的客户端连接请求。
//! - `client_session`:      定义了代表单个已连接 WebSocket 客户端的会话对象 (`ClientSession`)，
//!                        封装了客户端的唯一标识、网络流、状态信息（如最后活跃时间、所属组、角色等）。
//! - `connection_manager`:  (P1.2.1 引入) 核心组件，负责管理所有活跃的 `ClientSession` 实例，
//!                        处理客户端的加入和移除，以及客户端组的管理 (P3.1.1 增强)。
//! - `message_router`:      (P1.3.1 引入) 负责解析从客户端接收到的 `WsMessage`，并根据消息类型
//!                        将其路由到相应的处理逻辑或业务模块。也负责将业务逻辑的结果封装成
//!                        `WsMessage` 发送回客户端或广播给特定组。
//! - `heartbeat_monitor`:   (P1.4.1 引入, P3.2.1 增强) 后台服务，定期检查所有客户端的活跃状态，
//!                        并移除超时的客户端连接，以维护系统健康。
//! - `task_state_manager`:  (P3.1.2/P3.3.1 引入) 负责在云端维护和管理进行中的调试任务的共享状态数据模型
//!                        (`TaskDebugState`)，并提供接口供 `MessageRouter` 更新和查询这些状态。
//!
//! (规划中) 未来可能还会包含：
//! - `data_synchronizer` (或类似名称，对应 P3.3 DataHub 的概念): 
//!   更专注于业务数据同步逻辑的模块，可能与 `task_state_manager` 紧密协作，
//!   或者 `task_state_manager` 本身就承担了这部分职责，取决于最终实现。

// 公开导出 WebSocket 服务的主要组件，以便 `main.rs` 或其他顶层模块可以使用。
pub mod service;
pub mod client_session;
pub mod connection_manager;
pub mod message_router;
pub mod heartbeat_monitor;
pub mod task_state_manager;

// 预留注释：如果未来需要定义仅在 `ws_server` 模块内部使用的类型或辅助模块，
// 可以在这里声明为非 `pub` 的模块，例如：
// mod internal_types; 
// 或者，如果某些子模块中的类型需要在 `ws_server` 模块级别被方便地重导出，
// 也可以在这里使用 `pub use` 语句。
// 例如: `pub use client_session::ClientRole;` (如果 ClientRole 定义在 client_session.rs 中) 