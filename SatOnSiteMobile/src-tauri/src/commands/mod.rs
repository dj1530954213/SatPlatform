// SatOnSiteMobile/src-tauri/src/commands/mod.rs

//! Tauri 命令处理模块。

pub mod general_cmds; // P2.1.1
pub mod task_cmds;    // P7.4.x
pub mod data_cmds;    // P5.2.1, P8.1.3
pub mod test_cmds;    // P4.2.1, P8.2.3, P9.2.3 等
pub mod mobile_cmds;  // P13.3.1 (如果需要) 

// 公开 general_cmds 中的所有 public 函数 (即 Tauri 命令)
pub use general_cmds::*; 

// 如果 SatOnSiteMobile 有其他命令模块，也在这里声明 