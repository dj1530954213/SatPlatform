// SatCloudService/src-tauri/src/ws_server/connection_manager.rs

//! WebSocket 连接管理。

use std::sync::Arc;
use std::net::SocketAddr;
use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;
use log::{info, debug, warn};
use tokio::sync::RwLock;

use crate::ws_server::client_session::ClientSession;
use rust_websocket_utils::message::WsMessage; // WsMessage 来自我们封装的库
use crate::ws_server::task_state_manager::TaskStateManager; // P3.1.2: 引入 TaskStateManager
use common_models::enums::ClientRole;
use common_models::ws_payloads::{
    RegisterPayload, RegisterResponsePayload, PARTNER_STATUS_UPDATE_MESSAGE_TYPE, PartnerStatusPayload,
};

/// `Group` 结构体定义
/// 
/// 代表一个客户端组，通常关联到一个特定的任务。
/// 组内可以包含不同角色的客户端，例如控制中心和现场移动端。
#[derive(Debug)]
pub struct Group {
    /// 组的唯一标识。
    pub group_id: String,
    /// 与此组关联的任务ID。
    /// 在组创建时设定，用于后续的任务状态管理。
    pub task_id: String,
    /// 组内的控制中心客户端会话。
    /// `Option<Arc<ClientSession>>` 表示控制中心客户端可能存在也可能不存在。
    /// 使用 `Arc` 是因为 `ClientSession` 也可能在 `ConnectionManager` 的主 `clients` 集合中被引用。
    pub control_center_client: Option<Arc<ClientSession>>,
    /// 组内的现场移动端客户端会话。
    pub on_site_mobile_client: Option<Arc<ClientSession>>,
    // /// (未来可能扩展) 其他类型的客户端或观察者。
    // pub other_clients: Vec<Arc<ClientSession>>,
}

impl Group {
    /// 创建一个新的 `Group` 实例。
    ///
    /// # Arguments
    ///
    /// * `group_id` - 组的唯一标识符。
    /// * `task_id` - 与此组关联的任务ID。
    ///
    /// # Returns
    ///
    /// 返回一个新的 `Group` 实例，初始时没有任何客户端。
    pub fn new(group_id: String, task_id: String) -> Self {
        Self {
            group_id,
            task_id,
            control_center_client: None,
            on_site_mobile_client: None,
            // other_clients: Vec::new(),
        }
    }
}

/// 管理所有活动的 WebSocket 客户端会话
#[derive(Debug, Clone)]
pub struct ConnectionManager {
    /// 存储所有活动的 ClientSession，使用 DashMap 实现线程安全
    /// Key: client_id (Uuid) - 由 ConnectionManager 生成的会话ID
    /// Value: Arc<ClientSession>
    pub clients: Arc<DashMap<Uuid, Arc<ClientSession>>>,
    /// 存储所有活动组的线程安全哈希映射。
    /// 键是组的唯一ID (`String`)，值是 `Group` 的线程安全读写锁保护的共享引用。
    /// 使用 `DashMap` 进行高效的并发访问。
    /// 使用 `Arc<RwLock<Group>>` 允许多个任务并发地读取或修改单个组的信息。
    pub groups: Arc<DashMap<String, Arc<RwLock<Group>>>>,
    pub task_state_manager: Arc<TaskStateManager>, // P3.1.2: 添加 TaskStateManager 引用
}

impl ConnectionManager {
    /// 创建一个新的 ConnectionManager 实例
    pub fn new(task_state_manager: Arc<TaskStateManager>) -> Self {
        info!("[ConnectionManager] New instance created.");
        Self {
            clients: Arc::new(DashMap::new()),
            groups: Arc::new(DashMap::new()),
            task_state_manager, // P3.1.2: 初始化 TaskStateManager 引用
        }
    }

    /// 添加一个新的客户端会话到管理器中。
    /// 此方法应由 TransportLayer 在接受新的 WebSocket 连接后调用。
    ///
    /// # Arguments
    /// * `addr` - 新连接客户端的 SocketAddr。
    /// * `sender` - 用于向该客户端发送消息的 mpsc::Sender。
    ///
    /// # Returns
    /// 返回新创建的 `Arc<ClientSession>`。
    pub async fn add_client(&self, addr: SocketAddr, sender: mpsc::Sender<WsMessage>) -> Arc<ClientSession> {
        let client_id = Uuid::new_v4(); // 生成唯一的会话ID
        let client_session = Arc::new(ClientSession::new(client_id, sender, addr));
        
        self.clients.insert(client_id, Arc::clone(&client_session));
        
        info!("新客户端连接成功: id={}, addr={}, 初始角色={:?}", 
              client_session.client_id, client_session.addr, *client_session.role.read().await);
        debug!("客户端会话详情 (添加时): {:?}", client_session);
        debug!("当前活动客户端总数: {}", self.clients.len());

        client_session
    }

    /// 根据 client_id 获取一个客户端会话的引用。
    ///
    /// # Arguments
    /// * `client_id` - 要查找的客户端的 Uuid。
    ///
    /// # Returns
    /// 如果找到，则返回 `Some(Arc<ClientSession>)`，否则返回 `None`。
    pub async fn get_client(&self, client_id: &Uuid) -> Option<Arc<ClientSession>> {
        self.clients.get(client_id).map(|entry| Arc::clone(entry.value()))
    }
    
    /// 从管理器中移除一个客户端会话。
    /// 
    /// 此方法应在客户端连接断开时由 TransportLayer (或其回调) 调用。
    ///
    /// # Arguments
    /// * `client_id` - 要移除的客户端的 Uuid (由 ConnectionManager 生成的会话ID)。
    ///
    /// # Returns
    /// 如果找到并成功移除了会话，则返回被移除的 `Arc<ClientSession>`，否则返回 `None`。
    pub async fn remove_client(&self, client_id: &Uuid) -> Option<Arc<ClientSession>> {
        match self.clients.remove(client_id) {
            Some((_id, session)) => {
                // 为了日志，克隆必要的信息或在独立的块中读取，以确保锁在 session 被移出前释放
                let client_id_for_log = session.client_id; // Uuid 是 Clone 的
                let addr_for_log = session.addr;         // SocketAddr 是 Clone 的
                let role_val_for_log = {
                    let role_guard = session.role.read().await;
                    role_guard.clone() // ClientRole 是 Clone 的
                }; // role_guard (读锁) 在此代码块结束时被释放

                info!(
                    "客户端断开连接: id={}, addr={}, 角色={:?}",
                    client_id_for_log,
                    addr_for_log,
                    role_val_for_log
                );
                // P3.1.3: 如果客户端属于某个组，则通知组内伙伴此客户端下线。
                // 此处可以添加对 session.group_id 的检查和处理逻辑。

                debug!("移除后当前活动客户端总数: {}", self.clients.len());
                Some(session) // 现在可以安全地移动 session
            }
            None => {
                log::warn!(
                    "尝试移除不存在的客户端: id={}",
                    client_id
                );
                None
            }
        }
    }

    /// 处理客户端加入组的请求。
    ///
    /// # Arguments
    /// * `client_session` - 请求加入组的客户端会话。
    /// * `payload` - 包含 `group_id`, `role`, `task_id` 的注册负载。
    ///
    /// # Returns
    /// * `Ok(RegisterResponsePayload)` - 如果成功加入或创建组。
    /// * `Err(RegisterResponsePayload)` - 如果发生错误（例如角色冲突）。
    pub async fn join_group(
        &self,
        client_session: Arc<ClientSession>,
        payload: RegisterPayload,
    ) -> Result<RegisterResponsePayload, RegisterResponsePayload> {
        let group_id = payload.group_id;
        let requested_role = payload.role;
        let task_id = payload.task_id;
        let client_id = client_session.client_id;

        info!(
            "[ConnectionManager] 客户端 {} 请求加入组 '{}' (任务 '{}')，角色为: {:?}",
            client_id, group_id, task_id, requested_role
        );

        // 1. 组存在性与创建，并获取组的写锁
        // 使用 DashMap 的 entry API 来原子性地获取或插入
        let group_entry = self.groups.entry(group_id.clone()).or_insert_with(|| {
            info!(
                "[ConnectionManager] 组 '{}' (任务 '{}') 不存在，将创建新组。",
                group_id, task_id
            );
            // P3.1.2: 在创建新组时，也应调用 TaskStateManager 初始化任务状态
            // 由于这是在 or_insert_with 闭包内，且 task_state_manager.init_task_state 是 async, 
            // 我们不能直接 await。因此，这个调用需要移到获取锁之后，或者 TaskStateManager::new group 时同步处理（如果可能）
            // 为了简单起见，并且考虑到 init_task_state 的幂等性，我们在获得锁后，如果判断是新创建的组，再调用。
            // 这里暂时只创建 Group 结构。
            Arc::new(RwLock::new(Group::new(group_id.clone(), task_id.clone())))
        });
        
        let group_arc_rwlock = group_entry.value().clone(); // 获取 Arc<RwLock<Group>>
        let mut group_guard = group_arc_rwlock.write().await; // 获取组的写锁

        // 检查是否是新创建的组，如果是，则调用 init_task_state
        // 这是一个简化的判断，如果组刚被插入，其成员应为空
        let is_newly_created_group = group_guard.control_center_client.is_none() && group_guard.on_site_mobile_client.is_none();
        if is_newly_created_group {
             // 在创建新组时，也应调用 TaskStateManager 初始化任务状态
            self.task_state_manager.init_task_state(group_id.clone(), task_id.clone()).await;
            info!(
                "[ConnectionManager] 新组 '{}' 已创建，并已调用 TaskStateManager::init_task_state。",
                group_id
            );
        }


        // 2. 角色冲突检查
        match requested_role {
            ClientRole::ControlCenter => {
                if let Some(existing_cc) = &group_guard.control_center_client {
                    if existing_cc.client_id != client_id {
                        warn!(
                            "[ConnectionManager] 角色冲突：客户端 {} 尝试以 ControlCenter 加入组 '{}'，但该组已存在 ControlCenter (ID: {}).",
                            client_id, group_id, existing_cc.client_id
                        );
                        return Err(RegisterResponsePayload {
                            success: false,
                            message: Some(format!("组 '{}' 已存在控制中心客户端。", group_id)),
                            assigned_client_id: client_id,
                            effective_group_id: None,
                            effective_role: None,
                        });
                    }
                    // 如果是同一个客户端重复注册（理论上不应发生，因为注册是连接后的第一步），则允许通过
                    info!(
                        "[ConnectionManager] 客户端 {} (ControlCenter) 重新确认在组 '{}' 中的身份。",
                        client_id, group_id
                    );
                }
            }
            ClientRole::OnSiteMobile => {
                if let Some(existing_osm) = &group_guard.on_site_mobile_client {
                    if existing_osm.client_id != client_id {
                        warn!(
                            "[ConnectionManager] 角色冲突：客户端 {} 尝试以 OnSiteMobile 加入组 '{}'，但该组已存在现场移动端 (ID: {}).",
                            client_id, group_id, existing_osm.client_id
                        );
                        return Err(RegisterResponsePayload {
                            success: false,
                            message: Some(format!("组 '{}' 已存在现场移动端客户端。", group_id)),
                            assigned_client_id: client_id,
                            effective_group_id: None,
                            effective_role: None,
                        });
                    }
                    info!(
                        "[ConnectionManager] 客户端 {} (OnSiteMobile) 重新确认在组 '{}' 中的身份。",
                        client_id, group_id
                    );
                }
            }
            ClientRole::Unknown => {
                warn!(
                    "[ConnectionManager] 客户端 {} 尝试以 Unknown 角色加入组 '{}'。拒绝请求。",
                    client_id, group_id
                );
                return Err(RegisterResponsePayload {
                    success: false,
                    message: Some("不允许以 'Unknown' 角色加入组。".to_string()),
                    assigned_client_id: client_id,
                    effective_group_id: None,
                    effective_role: None,
                });
            }
        }

        // 3. 更新客户端会话状态
        {
            let mut role_guard = client_session.role.write().await;
            *role_guard = requested_role.clone();
            let mut group_id_guard = client_session.group_id.write().await;
            *group_id_guard = Some(group_id.clone());
        }
        info!(
            "[ConnectionManager] 客户端 {} 的会话状态已更新：角色 -> {:?}，组ID -> {}",
            client_id, requested_role, group_id
        );

        // 4. 更新组内成员信息 (在组的写锁 `group_guard` 内)
        match requested_role {
            ClientRole::ControlCenter => {
                group_guard.control_center_client = Some(client_session.clone());
            }
            ClientRole::OnSiteMobile => {
                group_guard.on_site_mobile_client = Some(client_session.clone());
            }
            ClientRole::Unknown => { /* 已在前面处理，此处不会到达 */ }
        }
        info!(
            "[ConnectionManager] 组 '{}' 的成员信息已更新，客户端 {} ({:?}) 已加入。",
            group_id, client_id, requested_role
        );

        // 5. P3.1.3: 此处将添加通知伙伴上线的逻辑。
        // (暂时先不实现，以免引入过多复杂性)

        // 6. 记录成功加入日志已在各处分散记录

        // 7. 准备并返回成功的 RegisterResponsePayload
        let success_response = RegisterResponsePayload {
            success: true,
            message: Some(format!("已成功加入组 '{}'。", group_id)),
            assigned_client_id: client_id,
            effective_group_id: Some(group_id.clone()),
            effective_role: Some(requested_role.clone()),
        };

        info!(
            "[ConnectionManager] 客户端 {} 成功加入组 '{}'。响应: {:?}",
            client_id, group_id, success_response
        );
        Ok(success_response)
    }

    pub fn get_client_count(&self) -> usize {
        self.clients.len()
    }

    // 用于测试或调试，获取一个组的只读访问权限
    #[allow(dead_code)] // 暂时允许未使用，因为主要用于测试或调试
    pub async fn get_group(&self, group_id: &str) -> Option<tokio::sync::OwnedRwLockReadGuard<Group>> {
        if let Some(group_entry) = self.groups.get(group_id) {
            let group_arc_rwlock = group_entry.value().clone();
            Some(group_arc_rwlock.read_owned().await)
        } else {
            None
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        // Default 需要一个 TaskStateManager, 这使得 Default 实现有点复杂
        // 通常，我们会移除 Default，或者让 TaskStateManager 也有 Default
        // 既然 TaskStateManager 已有 Default, 我们可以这样:
        Self::new(Arc::new(TaskStateManager::default()))
    }
} 