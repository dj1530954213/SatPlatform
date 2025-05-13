// SatOnSiteMobile/src-tauri/src/error.rs

//! `SatOnSiteMobile` (现场端移动应用) 的自定义错误处理模块。
//!
//! 本模块用于定义应用内部操作可能产生的特定错误类型。
//! 推荐使用 `thiserror` crate 来方便地创建具有良好上下文信息的错误枚举。
//!
//! 例如：
//! ```rust
//! use thiserror::Error;
//!
//! #[derive(Error, Debug)]
//! pub enum OnSiteError {
//!     #[error("设备通信失败: {0}")]
//!     DeviceCommunication(String),
//!
//!     #[error("配置文件解析错误: {source}")]
//!     ConfigParse { #[from] source: serde_json::Error },
//!
//!     #[error("WebSocket 消息发送失败: {reason}")]
//!     WebSocketSend { reason: String },
//!
//!     #[error("未知错误: {message}")]
//!     Unknown { message: String },
//! }
//! ```

// 初始占位：此模块当前为空。
// 后续开发中，当识别出需要特定错误类型来更清晰地表达失败场景时，
// 应在此处定义相应的错误枚举或结构体，并遵循项目错误处理规范。
// 例如，可以为设备通信、配置文件加载、API 调用等不同模块定义各自的错误类型。 