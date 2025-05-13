//! 通用枚举模块。
//!
//! 本模块定义了在 `SatPlatform` 项目中多个组件之间共享的通用枚举类型。
//! 这些枚举旨在提供类型安全，并确保对于如客户端角色、状态等概念在整个系统中有一致的表示。
//!
//! 所有在此模块中定义的枚举都应派生 `Serialize`, `Deserialize`, `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`
//! (如果适合作为 HashMap/HashSet 的键) 以支持数据交换、调试、实例复制、比较和集合操作。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 表示 WebSocket 客户端在系统中所扮演的角色。
/// 
/// 这个枚举用于区分不同类型的客户端（例如控制中心、现场移动端），
/// 以便服务器能够根据其角色应用不同的逻辑、权限或数据分发策略。
/// 它也用于在客户端之间进行识别和状态同步。
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientRole {
    /// 代表 `SatControlCenter` (控制中心) 类型的客户端。
    /// 通常负责任务的发起、监控、指令下发和全局状态的查看。
    ControlCenter,
    /// 代表 `SatOnSiteMobile` (现场移动端) 类型的客户端。
    /// 通常在现场执行具体任务，如预检查、单体测试步骤，并向云端和控制中心反馈状态和数据。
    OnSiteMobile,
    /// 代表一个角色尚未确定或未知的客户端。
    /// 这可能是客户端刚连接尚未完成注册流程时的初始状态，或者在某些错误/异常情况下使用。
    Unknown,
}

// 为 ClientRole 实现 Display trait
impl fmt::Display for ClientRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 使用 Debug 格式化，它已经为我们生成了枚举成员的名称字符串
        // 例如 ClientRole::ControlCenter 会变成 "ControlCenter"
        write!(f, "{:?}", self)
    }
}

/// 枚举：定义了预检查项的状态。
/// 
/// 根据规则 2.1，共享模型需派生 Serialize, Deserialize, Debug, Clone。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum PreCheckStatus {
    /// 待检查
    Pending,
    /// 通过
    Passed,
    /// 未通过
    Failed,
    /// 跳过/不适用
    Skipped, 
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    /// 测试 `ClientRole` 枚举成员的创建和等价性比较。
    fn test_client_role_creation_and_equality() {
        let role1 = ClientRole::ControlCenter;
        let role2 = ClientRole::ControlCenter;
        let role3 = ClientRole::OnSiteMobile;

        // 断言：两个 ControlCenter 实例应该相等
        assert_eq!(role1, role2, "ClientRole::ControlCenter 应该等于 ClientRole::ControlCenter");
        // 断言：ControlCenter 实例不应等于 OnSiteMobile 实例
        assert_ne!(role1, role3, "ClientRole::ControlCenter 不应等于 ClientRole::OnSiteMobile");
        // 断言：两个 Unknown 实例应该相等
        assert_eq!(ClientRole::Unknown, ClientRole::Unknown, "ClientRole::Unknown 应该等于 ClientRole::Unknown");
    }

    #[test]
    /// 测试 `ClientRole` 的 `Debug` 和 `Clone` trait 是否按预期工作。
    fn test_client_role_debug_clone() {
        let role = ClientRole::OnSiteMobile;
        let cloned_role = role.clone(); // 测试 Clone trait
        // 断言：克隆后的角色应与原始角色相等
        assert_eq!(role, cloned_role, "克隆后的 ClientRole::OnSiteMobile 实例应与原始实例相等");
        // 断言：Debug trait 的格式化输出应为枚举成员的名称字符串
        assert_eq!(format!("{:?}", role), "OnSiteMobile", "ClientRole::OnSiteMobile 的 Debug 输出格式不正确，应为 \"OnSiteMobile\"");
    }

    #[test]
    /// 测试 `ClientRole` 枚举的序列化 (到 JSON) 和反序列化 (从 JSON) 功能。
    fn test_client_role_serialization_deserialization() {
        let roles_to_test = vec![
            ClientRole::ControlCenter,
            ClientRole::OnSiteMobile,
            ClientRole::Unknown,
        ];

        for role_instance in roles_to_test {
            // 测试序列化
            let serialized_json = serde_json::to_string(&role_instance)
                .expect(&format!("ClientRole::{:?} 序列化到 JSON 失败", role_instance));
            
            // 测试反序列化
            let deserialized_role: ClientRole = serde_json::from_str(&serialized_json)
                .expect(&format!("从 JSON \"{}\" 反序列化 ClientRole 失败", serialized_json));
            
            // 断言：原始实例与经过序列化再反序列化得到的实例应相等
            assert_eq!(role_instance, deserialized_role, 
                       "对于 {:?}，序列化后再反序列化的实例与原始实例不匹配", role_instance);

            // 进一步验证序列化后的 JSON 字符串是否符合预期 (枚举值作为字符串)
            let expected_json_string = match role_instance {
                ClientRole::ControlCenter => "\"ControlCenter\"",
                ClientRole::OnSiteMobile => "\"OnSiteMobile\"",
                ClientRole::Unknown => "\"Unknown\"",
            };
            assert_eq!(serialized_json, expected_json_string, 
                       "对于 {:?}，序列化后的 JSON 字符串 \"{}\" 与预期的 \"{}\" 不符", 
                       role_instance, serialized_json, expected_json_string);
        }
    }

    #[test]
    /// 测试 `ClientRole` 枚举是否能正确地用作 `HashSet` 的元素，即验证 `Hash` 和 `Eq` trait 的实现。
    fn test_client_role_hash() {
        let mut roles_set = HashSet::new();
        roles_set.insert(ClientRole::ControlCenter); // 插入 ControlCenter
        roles_set.insert(ClientRole::ControlCenter); // 再次插入 ControlCenter，由于 HashSet 的特性，集合大小不应改变
        roles_set.insert(ClientRole::OnSiteMobile);  // 插入 OnSiteMobile

        // 断言：HashSet 中应包含两个唯一的角色
        assert_eq!(roles_set.len(), 2, "HashSet 中应包含2个唯一的 ClientRole 成员");
        // 断言：HashSet 中应包含 ControlCenter
        assert!(roles_set.contains(&ClientRole::ControlCenter), "HashSet 中应包含 ClientRole::ControlCenter");
        // 断言：HashSet 中应包含 OnSiteMobile
        assert!(roles_set.contains(&ClientRole::OnSiteMobile), "HashSet 中应包含 ClientRole::OnSiteMobile");
        // 断言：HashSet 中不应包含 Unknown (因为我们没有插入它)
        assert!(!roles_set.contains(&ClientRole::Unknown), "HashSet 中不应包含 ClientRole::Unknown");
    }
} 