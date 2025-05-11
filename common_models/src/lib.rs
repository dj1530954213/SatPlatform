//! `common_models` 公共模型库 crate。
//!
//! 本 crate 集中定义了在 `SatPlatform` 项目各个 Rust 组件（如 `SatCloudService` 云端服务、
//! `SatControlCenter` 控制中心桌面应用的 Tauri 后端、`SatOnSiteMobile` 现场移动端桌面应用的 Tauri 后端）
//! 以及潜在的 Web 前端（通过 TypeScript 类型对应）之间共享的核心数据结构和枚举类型。
//!
//! 主要包含以下类型的模型：
//! - **项目详情 (`project_details`)**: 与项目相关的元数据和配置信息。
//! - **任务信息 (`task_info`)**: 包含任务的详细描述、状态、预检查项、测试步骤等。
//! - **WebSocket 消息负载 (`ws_payloads`)**: 用于客户端与服务端之间通过 WebSocket 通信时传输的各类消息的 Payload 结构体，
//!   例如注册、Echo、Ping/Pong、伙伴状态更新、任务状态更新等。
//! - **通用枚举 (`enums`)**: 定义了项目中广泛使用的枚举类型，如客户端角色 (`ClientRole`)、任务状态等，以保证类型安全和一致性。
//!
//! 设计原则：
//! - **共享性**: 所有在此 crate 中定义的模型都旨在被多个其他 crate 共享使用。
//! - **序列化/反序列化**: 所有模型（结构体和枚举）都必须派生 `serde::Serialize` 和 `serde::Deserialize` traits，
//!   以便能够轻松地在不同格式（如 JSON）之间进行转换，这对于网络通信和持久化至关重要。
//! - **可调试性与克隆**: 所有模型也必须派生 `Debug` 和 `Clone` traits，以方便调试输出和创建副本。
//! - **一致性**: 通过统一管理这些共享模型，确保了跨不同语言边界（Rust <-> TypeScript）和不同服务边界时数据结构的一致性。

// 声明并公开项目中的各个模块
pub mod project_details;    // 与项目配置和元数据相关的模型
pub mod task_info;          // 与调试任务详细信息（状态、步骤等）相关的模型
pub mod ws_payloads;        // WebSocket 通信中使用的各种消息负载结构体
pub mod enums;              // 项目中通用的枚举类型定义

/// 一个简单的示例函数，用于演示 crate 的基本功能和测试。
/// 在实际的 `common_models` 库中，此类通用工具函数可能较少，主要侧重于数据结构定义。
///
/// # Arguments
/// * `left` - 左操作数 (u64类型)。
/// * `right` - 右操作数 (u64类型)。
///
/// # Returns
/// 返回两个参数的和 (u64类型)。
pub fn add(left: u64, right: u64) -> u64 {
    left + right // 执行加法操作
}

// 单元测试模块
#[cfg(test)]
mod tests {
    use super::*; // 导入父模块（即本 crate 的根）的所有公共项

    // 一个简单的测试用例，验证 `add` 函数是否按预期工作。
    #[test]
    fn it_works() {
        let result = add(2, 2); // 调用 add 函数
        assert_eq!(result, 4);   // 断言结果是否等于 4
    }
}
