// SatCloudService/src-tauri/src/api/mod.rs

//! API 请求处理模块。
//! 
//! 本模块负责定义和组织所有与外部 HTTP API 请求相关的处理逻辑。
//! 它通常会包含多个子模块，每个子模块对应一组相关的 API 路由和处理器 (handler)。
//! 例如，`task_handler` 用于处理与任务管理相关的 API 请求。

// 声明 `task_handler` 子模块，该模块包含了任务相关 API 的具体实现。
pub mod task_handler;

// 预留注释：后续随着项目功能的扩展，可能会在这里添加更多的 handler 子模块，
// 例如：
// pub mod project_handler; // 用于处理项目管理相关的 API
// pub mod user_handler;    // 用于处理用户认证和管理相关的 API
// 等等。 