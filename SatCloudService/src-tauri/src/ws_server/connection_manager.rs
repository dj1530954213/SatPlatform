// SatCloudService/src-tauri/src/ws_server/connection_manager.rs

//! WebSocket 连接管理。

use std::sync::Arc;
use std::net::SocketAddr;
use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;
use log::{info, debug};

use crate::ws_server::client_session::ClientSession;
use rust_websocket_utils::message::WsMessage; // 修改这里

/// 管理所有活动的 WebSocket 客户端会话
#[derive(Debug)]
pub struct ConnectionManager {
    /// 存储所有活动的 ClientSession，使用 DashMap 实现线程安全
    /// Key: client_id (Uuid)
    /// Value: Arc<ClientSession>
    clients: Arc<DashMap<Uuid, Arc<ClientSession>>>,
}

impl ConnectionManager {
    /// 创建一个新的 ConnectionManager 实例
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
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
        let client_id = Uuid::new_v4();
        let client_session = Arc::new(ClientSession::new(client_id, sender, addr));
        
        self.clients.insert(client_id, Arc::clone(&client_session));
        
        info!("New client connected: id={}, addr={}, initial_role={:?}", 
              client_session.client_id, client_session.addr, *client_session.role.read().await);
        debug!("ClientSession details on add: {:?}", client_session);
        debug!("Total active clients: {}", self.clients.len());

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
    
    // P1.2.2: remove_client 将在这里实现
    // P3.1.1: 组管理相关方法将在这里实现
    // P3.2.1: 与心跳检查配合的方法可能在这里实现 (如 get_all_clients)
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
} 