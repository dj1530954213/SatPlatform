// SatControlCenter/src-tauri/src/commands/mod.rs

//! Tauri 命令处理模块。

pub mod general_cmds; // P2.1.1
pub mod task_cmds;    // P7.3.4
pub mod data_cmds;    // P5.2.1
pub mod test_cmds;    // P4.2.1, P9.1.3 等 

// 可以选择性地重新导出命令，以便更容易地在 main.rs 中注册
// pub use general_cmds::connect_to_cloud; 