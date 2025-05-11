// SatCloudService/src-tauri/src/state.rs

//! Tauri 应用全局共享状态管理模块。
//! 
//! 此模块用于定义和管理那些需要在 Tauri 应用的不同部分（例如，多个窗口之间、
//! Rust 后端命令与前端之间，或者后台任务与主应用逻辑之间）共享的数据和状态。
//! 
//! Tauri 提供了 `tauri::State<T>` 机制来安全地管理和访问共享状态。
//! 通常，我们会在这里定义代表共享状态的结构体，然后在 `main.rs` 的 `setup`
//! 钩子中创建这些状态的实例，并使用 `app.manage()` 将它们注册到 Tauri 的状态管理器中。
//! 之后，在 Tauri 命令处理函数中，可以通过类型声明来获取对这些托管状态的引用。
//! 
//! 例如：
//! ```rust,ignore
//! use std::sync::Mutex;
//! pub struct MySharedState {
//!     pub counter: Mutex<i32>,
//! }
//! 
//! // 在 main.rs setup 中:
//! // app.manage(MySharedState { counter: Mutex::new(0) });
//! 
//! // 在 Tauri command 中:
//! // #[tauri::command]
//! // fn increment_counter(state: tauri::State<MySharedState>) {
//! //     let mut count = state.counter.lock().unwrap();
//! //     *count += 1;
//! // }
//! ```
//
// 当前，诸如 `ConnectionManager` 和 `TaskStateManager` 等核心共享状态，
// 是在其各自的模块中定义，并在 `main.rs` 中创建实例后直接通过 `app.manage()` 进行管理的。
// 此 `state.rs` 文件可以作为未来集中定义更多纯粹数据型共享状态的地方，
// 或者用于定义那些不紧密耦合于特定服务模块的全局状态。

// 暂时为空，后续会定义和管理 Tauri 应用状态。 