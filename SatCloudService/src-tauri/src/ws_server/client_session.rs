use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use std::net::SocketAddr;
use chrono::{DateTime, Utc};
use common_models::enums::ClientRole;
use rust_websocket_utils::message::WsMessage; // 修改这里
use std::sync::atomic::AtomicBool;

/// 代表一个已连接的 WebSocket 客户端会话
#[derive(Debug)]
pub struct ClientSession {
    /// 由服务端生成的唯一客户端标识
    pub client_id: Uuid,
    /// 客户端的角色，使用 RwLock 实现线程安全的可变性
    pub role: Arc<RwLock<ClientRole>>,
    /// 用于向此客户端异步发送 WsMessage 的通道发送端
    pub sender: mpsc::Sender<WsMessage>,
    /// 客户端的 IP 地址和端口
    pub addr: SocketAddr,
    /// 会话创建的时间戳
    pub creation_time: DateTime<Utc>,
    /// 客户端最后活跃时间戳，用于心跳机制，使用 RwLock 实现线程安全更新
    pub last_seen: Arc<RwLock<DateTime<Utc>>>,
    /// 客户端所属的组ID (为P3.1.1准备)
    pub group_id: Arc<RwLock<Option<String>>>,
    pub connection_should_close: Arc<AtomicBool>,
}

impl ClientSession {
    /// 创建一个新的 ClientSession 实例
    pub fn new(
        addr: SocketAddr,
        sender: mpsc::Sender<WsMessage>,
        connection_should_close: Arc<AtomicBool>,
    ) -> Self {
        let client_id = Uuid::new_v4();
        let now = Utc::now();
        Self {
            client_id,
            role: Arc::new(RwLock::new(ClientRole::Unknown)),
            sender,
            addr,
            creation_time: now,
            last_seen: Arc::new(RwLock::new(now)),
            group_id: Arc::new(RwLock::new(None)),
            connection_should_close,
        }
    }
} 