// SatOnSiteMobile/src-tauri/src/commands/mod.rs

//! `SatOnSiteMobile` (现场端移动应用) 的 Tauri 命令模块根文件。
//!
//! 本模块 (`commands`) 聚合了应用中所有可由前端通过 `invoke` 调用的 Rust 函数 (即 Tauri 命令)。
//! 每个子模块通常按功能或相关性组织一组命令。
//!
//! ## 结构与约定：
//! - 子模块 (例如 `general_cmds`, `task_cmds`) 负责定义具体的 `#[tauri::command]` 函数。
//! - 此 `mod.rs` 文件通过 `pub mod` 声明这些子模块，使其对 crate 可见。
//! - 为了方便在 `main.rs` 中通过 `tauri::generate_handler!` 宏注册所有命令，
//!   通常会 `pub use` 子模块中的命令，或者子模块本身就是 `pub` 的（如果命令直接在子模块中声明为 `pub`）。
//!   当前实现选择 `pub use general_cmds::*;` 作为示例，具体导出策略可根据项目规范调整。

// --- 子模块声明 --- 

/// 通用命令模块 (项目阶段 P2.1.1 引入)。
/// 包含与应用核心功能相关的通用命令，例如连接/断开 WebSocket、发送 Echo 等。
pub mod general_cmds;

/// 任务相关操作命令模块 (项目阶段 P7.4.x 引入)。
/// 例如：获取任务列表、选择当前任务等（如果不由WebSocket直接管理）。
pub mod task_commands;

/// 数据管理与同步相关命令模块 (项目阶段 P5.2.1, P8.1.3 引入)。
/// 例如：手动触发数据上传、下载配置等。
pub mod data_cmds; 

/// 测试流程控制与反馈相关命令模块 (项目阶段 P4.2.1, P8.2.3, P9.2.3 等引入)。
/// 例如：启动/停止测试步骤、发送测试步骤反馈、确认预检查项等。
pub mod test_cmds;

/// 移动端特定功能命令模块 (项目阶段 P13.3.1 引入，如果需要)。
/// 例如：访问设备原生 API（相机、GPS）、处理移动端特有的用户交互等。
pub mod mobile_cmds;

// --- 公开导出 (Re-export) --- 
// 为了方便在 `main.rs` 中使用 `tauri::generate_handler!` 宏一次性注册来自 `general_cmds` 模块的所有命令，
// 我们在这里将 `general_cmds` 模块中的所有公共项（通常是 `#[tauri::command]` 函数）重新导出。
// 对于其他命令模块 (task_cmds, data_cmds, 等)，如果也希望采用类似的便捷注册方式，
// 可以取消注释或添加相应的 `pub use ...::*;` 语句。
// 或者，在 `main.rs` 的 `generate_handler!` 中分别列出每个模块的命令：
// `tauri::generate_handler![general_cmds::my_cmd, task_cmds::another_cmd]`
pub use general_cmds::*; 

pub use task_commands::*;

// 提示：如果 `SatOnSiteMobile` 应用未来引入更多类别的命令，
// 请在此处添加相应的 `pub mod` 声明，并在需要时调整导出策略。

// 如果 SatOnSiteMobile 有其他命令模块，也在这里声明 