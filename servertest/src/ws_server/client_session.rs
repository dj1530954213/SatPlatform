use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use std::net::SocketAddr;
use chrono::{DateTime, Utc};
use common_models::enums::ClientRole;
use rust_websocket_utils::message::WsMessage;
use std::sync::atomic::AtomicBool;

/// 代表一个已连接到服务器的 WebSocket 客户端的会话状态及相关句柄。
///
/// 每个成功建立的 WebSocket 连接都会在服务端对应一个 `ClientSession` 实例。
/// 此结构体封装了客户端的唯一标识、网络信息、角色、所属组、通信信道
/// 以及用于管理其生命周期的状态（如最后活跃时间、连接关闭标志等）。
///
/// 大部分可变状态（如 `role`, `last_seen`, `group_id`）都通过 `Arc<RwLock<T>>`
/// 进行包装，以允许多个异步任务安全地读取或修改这些状态。
#[derive(Debug)]
pub struct ClientSession {
    /// 由服务端在此会话成功创建时生成的、全局唯一的客户端标识符 (UUID 版本 4)。
    /// 此 ID 用于在整个系统中唯一地识别这个客户端连接。
    pub client_id: Uuid,

    /// 客户端在系统中扮演的角色 (例如：控制中心 `ClientRole::ControlCenter`、
    /// 现场移动端 `ClientRole::OnSiteMobile` 等)。
    /// 使用 `Arc<RwLock<...>>` 包装，以实现线程安全的可变性。
    /// 此角色在客户端会话创建时默认为 `ClientRole::Unknown`，
    /// 并在客户端成功通过 "Register" 消息加入一个组后，由 `ConnectionManager` 更新。
    pub role: Arc<RwLock<ClientRole>>,

    /// Tokio MPSC (多生产者单消费者) 通道的发送端 (`Sender`)。
    /// 此 `sender`专门用于异步地向这个特定的客户端发送 `WsMessage` 类型的 WebSocket 消息。
    /// 其他模块（如 `MessageRouter` 或 `ConnectionManager`）可以通过此 `sender` 将消息
    /// 推送到一个内部队列。WebSocket 连接的发送任务会从该队列中取出消息并实际发送到客户端。
    pub sender: mpsc::Sender<WsMessage>,

    /// 客户端 WebSocket 连接的源网络地址，包含其 IP 地址和端口号。
    /// 例如：`127.0.0.1:54321`。
    pub addr: SocketAddr,

    /// 此客户端会话在服务端被成功创建的时间戳 (使用协调世界时 UTC)。
    /// 此时间戳记录了连接建立的精确时刻。
    pub creation_time: DateTime<Utc>,

    /// 记录服务端最后一次收到该客户端任何有效消息的时间戳 (UTC 时间)。
    /// 此字段主要由心跳机制 (`HeartbeatMonitor`) 和消息处理逻辑 (`MessageRouter`) 更新。
    /// 每当收到客户端的 Ping 消息或其他业务消息时，应更新此时间戳。
    /// `HeartbeatMonitor` 会定期检查此值以判断客户端是否超时。
    /// 使用 `Arc<RwLock<...>>` 包装，以实现线程安全的更新。
    pub last_seen: Arc<RwLock<DateTime<Utc>>>,

    /// 客户端当前所属的调试任务组的ID (字符串类型)。
    /// 如果客户端尚未加入任何组，或者已从组中离开，则此值为 `None`。
    /// 此状态在客户端通过 "Register" 消息成功加入一个组后，由 `ConnectionManager` 设置。
    /// 使用 `Arc<RwLock<...>>` 包装，以实现线程安全的可变性。
    pub group_id: Arc<RwLock<Option<String>>>,

    /// 一个原子布尔标志，用于从外部（例如 `ConnectionManager` 在处理客户端移除时，
    /// 或 `HeartbeatMonitor` 在检测到客户端超时时）向处理此客户端连接的
    /// I/O 任务发出信号，指示其应优雅地关闭底层的 WebSocket 连接。
    /// 
    /// - 当值为 `true` 时，表示连接应当被关闭。
    /// - 当值为 `false` (默认值) 时，表示连接可以继续保持。
    /// 
    /// 此标志通常由管理连接生命周期的模块设置，而被连接的读写循环任务会定期检查它。
    pub connection_should_close: Arc<AtomicBool>,
}

impl ClientSession {
    /// 创建一个新的 `ClientSession` 实例。
    ///
    /// 此构造函数用于在 WebSocket 握手成功后，为新接受的客户端连接初始化一个会话对象。
    /// 它会生成一个唯一的 `client_id`，记录当前的创建和最后活跃时间，
    /// 并将角色和组ID初始化为默认状态。
    ///
    /// # 参数
    /// * `addr`: `SocketAddr` - 新连接客户端的网络源地址（IP和端口）。
    /// * `sender`: `mpsc::Sender<WsMessage>` - 一个 Tokio MPSC 通道的发送端，
    ///   用于将出站 WebSocket 消息发送给此客户端的专用发送任务。
    /// * `connection_should_close`: `Arc<AtomicBool>` - 一个共享的原子布尔标志，
    ///   允许其他部分（如连接管理器或心跳监视器）请求关闭此客户端的连接。
    ///
    /// # 返回
    /// 返回一个初始化完成的 `ClientSession` 实例。
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