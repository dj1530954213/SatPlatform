// SatControlCenter/src-tauri/src/commands/mod.rs

//! `SatControlCenter` (卫星控制中心) 应用的 Tauri 后端命令定义与组织模块。
//!
//! 本模块 (`commands`) 是所有可供前端 Angular 应用通过 `invoke` API 调用的 Tauri 后端命令的
//! 中心枢纽。它的主要职责是：
//! - **组织命令**: 将功能上相关的 Tauri 命令组织到不同的子模块中，以保持代码的清晰度和可维护性。
//!   例如，通用命令、任务相关命令、数据操作命令、测试命令等都分别放在各自的子模块里。
//! - **提供统一入口**: 作为 `main.rs` (或 `lib.rs` 中的 `run` 函数) 注册 Tauri 命令处理器时的主要入口点。
//!   通过 `tauri::generate_handler!` 宏，可以方便地将此模块及其子模块中定义的命令集体注册到 Tauri 应用中。
//!
//! # Tauri 命令简介
//! Tauri 命令是使用 `#[tauri::command]` 宏标记的 Rust 函数。这些函数可以被前端 JavaScript/TypeScript 代码
//! 通过 `@tauri-apps/api/core` 中的 `invoke("command_name", {{ args }})` 函数异步调用。
//! - **参数与返回值**: 命令函数可以接受参数 (需要实现 `serde::Deserialize`)，并且通常返回 `Result<T, String>`，
//!   其中 `T` 是成功时返回给前端的数据 (需要实现 `serde::Serialize`)，`String` 则是发生错误时返回给前端的
//!   用户友好的错误消息。这符合项目规则 (4.3. Tauri Command Design - Tauri命令设计)。
//! - **状态访问**: 命令可以访问通过 `app.manage()` 注册的 Tauri 托管状态，例如应用配置 (`AppConfig`)、
//!   `WebSocketClientService` 实例等，通过在函数签名中声明 `state: tauri::State<YourStateType>` 参数即可。
//! - **异步执行**: 命令可以是异步函数 (`async fn`)，允许在其中执行耗时的 I/O 操作 (如网络请求、文件读写)
//!   而不会阻塞 Tauri 主线程。

/// 子模块：`general_cmds` (通用命令)
/// 包含应用级别的通用控制命令，例如：
/// - 连接/断开与云端 WebSocket 服务的连接 (P2.1.1)。
/// - 检查 WebSocket 连接状态。
/// - 发送 Echo (回声) 测试消息等。
pub mod general_cmds;

/// 子模块：`task_cmds` (任务相关命令)
/// (预期功能，根据项目阶段 P7.3.4 规划)
/// 包含与任务管理相关的命令，例如：
/// - 获取任务列表。
/// - 创建/更新/删除任务。
/// - 控制任务执行 (开始/暂停/停止) 等。
pub mod task_cmds;

/// 子模块：`data_cmds` (数据操作命令)
/// (预期功能，根据项目阶段 P5.2.1 规划)
/// 可能包含与更通用的数据获取、存储或处理相关的命令，例如：
/// - 通过 `api_client` 模块调用外部API获取数据。
/// - 与本地数据存储 (如果未来引入) 交互等。
pub mod data_cmds;

/// 子模块：`test_cmds` (测试与诊断命令)
/// (预期功能，根据项目阶段 P4.2.1, P9.1.3 等规划)
/// 包含用于测试、诊断或开发辅助的命令，例如：
/// - 触发特定的测试场景。
/// - 获取内部状态或调试信息等。
/// 这些命令可能不会在最终的生产版本中暴露给普通用户。
pub mod test_cmds;

// --- 关于命令重新导出的说明与示例 --- 
// 在某些情况下，为了简化 `main.rs` 中 `tauri::generate_handler!` 宏的使用，
// 可以选择性地将各个子模块中的具体命令函数重新导出到 `commands` 模块的顶层命名空间。
// 例如，如果 `general_cmds` 模块中有一个名为 `connect_to_cloud` 的命令，
// 并且我们希望在 `main.rs` 中直接通过 `commands::connect_to_cloud` 来引用它 (而不是 `commands::general_cmds::connect_to_cloud`)，
// 就可以取消下面这行注释：
//
// pub use general_cmds::connect_to_cloud; // 示例：重新导出 `connect_to_cloud` 命令
//
// 这样做的好处是当 `generate_handler!` 宏的参数列表较长时，可以稍微缩短每个命令的路径。
// 但缺点是可能会使得 `commands` 模块的顶层命名空间显得有些拥挤，尤其当命令数量很多时。
// 是否采用此模式取决于项目的具体偏好和命令的组织复杂度。
// 当前项目中，`main.rs` 直接使用了子模块路径 (如 `commands::general_cmds::connect_to_cloud`)，
// 因此以下示例保持注释状态。
// pub use general_cmds::connect_to_cloud;