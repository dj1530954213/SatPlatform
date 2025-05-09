use serde::{Deserialize, Serialize};

/// 表示 WebSocket 客户端的角色
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientRole {
    /// 控制中心客户端
    ControlCenter,
    /// 现场移动端客户端
    OnSiteMobile,
    /// 未知或尚未注册角色的客户端
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_client_role_creation_and_equality() {
        let role1 = ClientRole::ControlCenter;
        let role2 = ClientRole::ControlCenter;
        let role3 = ClientRole::OnSiteMobile;

        assert_eq!(role1, role2, "ControlCenter should be equal to ControlCenter");
        assert_ne!(role1, role3, "ControlCenter should not be equal to OnSiteMobile");
        assert_eq!(ClientRole::Unknown, ClientRole::Unknown, "Unknown should be equal to Unknown");
    }

    #[test]
    fn test_client_role_debug_clone() {
        let role = ClientRole::OnSiteMobile;
        let cloned_role = role.clone();
        assert_eq!(role, cloned_role, "Cloned role should be equal to original");
        assert_eq!(format!("{:?}", role), "OnSiteMobile", "Debug format incorrect");
    }

    #[test]
    fn test_client_role_serialization_deserialization() {
        let roles = vec![
            ClientRole::ControlCenter,
            ClientRole::OnSiteMobile,
            ClientRole::Unknown,
        ];

        for role in roles {
            let serialized = serde_json::to_string(&role).expect("Serialization failed");
            let deserialized: ClientRole = serde_json::from_str(&serialized).expect("Deserialization failed");
            assert_eq!(role, deserialized, "Deserialized role should match original for {:?}", role);

            // 检查特定的序列化字符串是否符合预期 (例如，作为JSON字符串)
            let expected_json_str = match role {
                ClientRole::ControlCenter => "\"ControlCenter\"",
                ClientRole::OnSiteMobile => "\"OnSiteMobile\"",
                ClientRole::Unknown => "\"Unknown\"",
            };
            assert_eq!(serialized, expected_json_str, "Serialized JSON string mismatch for {:?}", role);
        }
    }

    #[test]
    fn test_client_role_hash() {
        let mut set = HashSet::new();
        set.insert(ClientRole::ControlCenter);
        set.insert(ClientRole::ControlCenter); // Insert duplicate, should not increase size
        set.insert(ClientRole::OnSiteMobile);

        assert_eq!(set.len(), 2, "HashSet should contain 2 unique roles");
        assert!(set.contains(&ClientRole::ControlCenter));
        assert!(set.contains(&ClientRole::OnSiteMobile));
        assert!(!set.contains(&ClientRole::Unknown));
    }
} 