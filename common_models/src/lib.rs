//! `common_models` crate 提供了在 SatPlatform 各个组件之间共享的核心数据结构。
//! 
//! 这些模型包括任务信息、项目详情、WebSocket 消息负载等，
//! 确保了跨 Rust 后端和前端 TypeScript 的数据一致性。
//! 所有共享模型都应实现 `Serialize`, `Deserialize`, `Debug`, `Clone` traits。

pub mod project_details;
pub mod task_info;
pub mod ws_payloads;
pub mod enums;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
