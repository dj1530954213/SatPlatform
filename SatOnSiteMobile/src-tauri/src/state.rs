// SatOnSiteMobile/src-tauri/src/state.rs

//! `SatOnSiteMobile` (现场端移动应用) 的共享状态管理模块。
//!
//! 本模块用于定义和管理那些需要在应用不同部分（尤其是在 Tauri 命令处理函数之间）
//! 共享的应用级别状态。
//!
//! Tauri 提供了内置的状态管理机制，允许将任何实现了 `Send + Sync + 'static` 的类型
//! 通过 `AppHandle::manage()` 方法注册为托管状态，之后可以在命令中通过 `tauri::State<T>` 进行访问。
//!
//! **主要应用场景**: 
//! - `WebSocketClientService`: 已经在 `main.rs` 的 `setup` 钩子中通过 `app.manage()` 注册。
//! - 其他需要跨命令共享的服务或数据结构，例如设备连接状态管理器、任务执行上下文等。
//!
//! 如果某些状态不适合或不需要通过 Tauri 的托管状态机制管理（例如，模块内部的私有状态），
//! 也可以在此模块中定义，并提供相应的访问和修改接口。
//!
//! ```rust
//! // 示例：定义一个简单的应用状态结构体
//! #[derive(Debug, Default)] // Default 可以方便地创建初始实例
//! pub struct AppMetrics {
//!     pub messages_sent: std::sync::atomic::AtomicUsize,
//!     pub messages_received: std::sync::atomic::AtomicUsize,
//! }
//!
//! // 在 main.rs 或 lib.rs 的 setup 中:
//! // app.manage(Arc::new(tokio::sync::Mutex::new(AppMetrics::default())));
//!
//! // 在命令中访问:
//! // async fn some_command(metrics: tauri::State<'_, Arc<tokio::sync::Mutex<AppMetrics>>>) -> Result<(), String> {
//! //     let mut metrics_guard = metrics.inner().lock().await;
//! //     metrics_guard.messages_sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
//! //     Ok(())
//! // }
//! ```

// 初始占位：此模块当前主要用于概念性说明。
// `WebSocketClientService` 实例已在 `main.rs` 中被 Tauri 管理。
// 后续如果需要其他全局共享状态，可以在此定义，并考虑是否也通过 Tauri 的状态管理机制进行注入。

// 暂时为空，后续会定义和管理 Tauri 应用状态。 