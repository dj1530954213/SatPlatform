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
}

impl ConnectionManager {
    /// 创建一个新的 ConnectionManager 实例
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            groups: Arc::new(DashMap::new()),
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

    // P3.1.1: 组管理相关方法将在这里实现
    // P3.2.1: 与心跳检查配合的方法可能在这里实现 (如 get_all_clients)
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
} 