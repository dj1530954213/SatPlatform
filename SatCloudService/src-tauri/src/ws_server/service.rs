// SatCloudService/src-tauri/src/ws_server/service.rs

//! WebSocket 服务端核心服务模块。
//!
//! 本模块的核心是 `WsService` 结构体及其 `start` 方法。`WsService` 负责：
//! - 根据提供的配置 (`WebSocketConfig`) 初始化服务。
//! - 使用 `rust_websocket_utils` 库的功能来启动一个底层的 WebSocket 服务器，该服务器会监听指定的网络地址和端口。
//! - 为每一个成功建立的客户端 WebSocket 连接，执行一个定制的回调逻辑 (`on_new_connection_cb`)。
//!
//! 在 `on_new_connection_cb` 回调中，针对每个新连接：
//! 1.  **会话创建**: 调用 `ConnectionManager::add_client` 来创建一个新的 `ClientSession` 实例。
//!     这个 `ClientSession` 代表了服务端对该客户端连接的完整状态认知，包括其唯一ID、网络地址（目前存在占位符问题，详见P3.1.1问题记录）、
//!     角色、所属组、最后活跃时间、以及一个用于向其发送消息的MPSC（多生产者单消费者）通道。
//! 2.  **双任务并发处理**: 派生两个独立的、并行的异步 Tokio 任务：
//!     a.  **发送任务 (`sender_task`)**: 此任务持有一个 MPSC 通道的接收端 (`rx_from_client_session`)。
//!         它不断地尝试从此通道接收 `WsMessage` (这些消息通常由 `MessageRouter` 或其他业务逻辑模块在处理完客户端请求后放入)。
//!         一旦收到消息，它会使用 `rust_websocket_utils` 提供的 `WsConnectionHandler` 将该消息异步发送到实际的客户端 WebSocket 连接。
//!         此任务也会监听 `ClientSession` 中的关闭信号 (`connection_should_close`)，并在信号置位或通道关闭时优雅地终止。
//!     b.  **接收与处理循环 (`receiver_loop`)**: 此任务持有 `rust_websocket_utils` 提供的 `SplitStream` (WebSocket 流的接收端)。
//!         它在一个循环中不断地尝试从客户端接收消息 (`receive_message`)。
//!         - 如果成功接收到消息，它会将该消息连同相关的 `ClientSession` 和 `ConnectionManager` 的共享引用，
//!           传递给 `message_router::handle_message` 函数进行进一步的路由和业务逻辑处理。
//!         - 此循环也实现了带超时的非阻塞接收，以能够周期性地检查 `ClientSession` 的关闭信号，并在信号置位、
//!           连接被对端关闭、或发生严重协议错误时优雅地终止。
//! 3.  **生命周期管理**: 通过 `ConnectionManager` 和 `HeartbeatMonitor` 间接管理客户端连接的生命周期。
//!     `MessageRouter` 在收到任何消息时会更新 `ClientSession::last_seen` (最后活跃时间)，而 `HeartbeatMonitor` (心跳监视器) 会定期检查此时间戳
//!     以移除超时的连接 (通过设置 `ClientSession::connection_should_close` (连接应关闭) 标志并调用 `ConnectionManager::remove_client` (移除客户端) )。
//!     上述的发送任务和接收循环都会响应这个关闭信号。
//!
//! 关键依赖与集成：
//! - `crate::config::WebSocketConfig`: 提供服务监听地址、端口等配置。
//! - `crate::ws_server::connection_manager::ConnectionManager`: 核心组件，负责管理所有 `ClientSession` (客户端会话)，处理客户端的加入、
//!   移除、组的创建与管理，以及伙伴状态通知等。
//! - `crate::ws_server::message_router`: 负责解析从客户端接收到的消息，并根据消息类型将其路由到相应的处理逻辑或业务模块。
//! - `rust_websocket_utils`: 公司内部封装的 WebSocket 工具库，提供了启动服务器、处理连接、发送/接收消息等底层传输功能。
//!   当前 `WsService` 与此库的集成点存在一些需要注意的增强点 (P3.1.1 问题记录中提及，并在代码中的 `TODO` 注释中有所体现)。

use crate::config::WebSocketConfig; // 引入应用内定义的 WebSocket 服务配置信息结构体。
use crate::ws_server::connection_manager::ConnectionManager; // 引入连接管理器，用于管理客户端会话和组。
use crate::ws_server::message_router; // 引入消息路由器模块，用于处理和分发收到的 WebSocket 消息。
use anyhow::{Context, Result}; // anyhow Crate (第三方包)，提供方便的错误处理和上下文添加功能。
use futures_util::stream::SplitStream; // futures-util Crate (第三方包) 的一部分，提供流 (Stream) 处理相关的工具，此处特指用于分离 WebSocket 流的读写部分。
use log::{debug, error, info, warn}; // 标准日志宏，用于在不同级别记录程序运行信息。
use rust_websocket_utils::{ // 从公司内部自定义的 `rust_websocket_utils` WebSocket 工具库导入所需组件。
    message::WsMessage as ActualWsMessage, // WebSocket 消息的标准结构体定义。使用 `as ActualWsMessage` 重命名是为了避免与项目中其他可能名为 `WsMessage` 的类型产生命名冲突，确保使用的是工具库中的定义。
    server::transport::{ // 从工具库的服务端传输层模块 (`server::transport`) 导入。
        start_server, // 一个函数，用于根据指定配置启动底层的 WebSocket 服务器并开始监听连接。
        ConnectionHandler as WsConnectionHandler, // 一个结构体或类型别名，封装了与单个已建立的 WebSocket 连接进行交互（主要是发送消息）的逻辑。
        receive_message, // 一个异步函数，用于从给定的 WebSocket 流（的接收端）尝试接收单个完整的消息。
    },
    error::WsError, // `rust_websocket_utils` 库定义的标准错误枚举类型，用于表示 WebSocket 操作中可能发生的各种错误。
};
use std::sync::Arc; // 标准库的原子引用计数类型 (`Arc`)，用于在多个线程或异步任务之间安全地共享对象所有权。
use tokio::sync::mpsc; // Tokio Crate (异步运行时) 提供的异步多生产者、单消费者 (MPSC) 通道，用于在异步任务间安全地传递消息。
use tokio_tungstenite::WebSocketStream; // Tokio 对 Tungstenite WebSocket 库的集成，表示一个异步的 WebSocket 连接流。
use tokio::net::TcpStream; // Tokio 提供的异步 TCP 流，是 WebSocket 连接的底层传输基础。

/// `WsService` (WebSocket 服务) 结构体定义。
///
/// 此结构体封装了运行 WebSocket 服务所需的核心状态和依赖项，主要包括：
/// - `config`: 服务的配置信息，如监听地址和端口。
/// - `connection_manager`: 对 `ConnectionManager` (连接管理器) 实例的共享引用 (`Arc`)。
///   `ConnectionManager` (连接管理器) 负责管理所有活动的客户端会话及其所属的组。
pub struct WsService {
    /// WebSocket 服务的具体配置信息，例如监听的主机地址和端口号。
    /// 这些配置通常在应用启动时从外部文件或环境变量加载。
    config: WebSocketConfig,
    
    /// 对全局 `ConnectionManager` (连接管理器) 实例的共享、线程安全的引用。
    /// `WsService` (WebSocket 服务) 通过此引用来注册新的客户端连接 (`add_client` - 添加客户端)，
    /// 而消息处理逻辑 (如 `message_router` - 消息路由器) 也可能需要通过此引用来查询或修改连接和组的状态。
    connection_manager: Arc<ConnectionManager>,
}

impl WsService {
    /// 创建一个新的 `WsService` (WebSocket 服务) 实例。
    ///
    /// 此构造函数接收服务运行所需的配置和对核心管理器的共享引用。
    ///
    /// # 参数
    /// * `config`: `WebSocketConfig` - WebSocket 服务的配置信息对象。
    /// * `connection_manager`: `Arc<ConnectionManager>` - 对 `ConnectionManager` (连接管理器) 实例的共享原子引用计数指针。
    ///   `ConnectionManager` (连接管理器) 应在调用此构造函数之前被创建和初始化。
    ///
    /// # 返回值
    /// 返回一个根据传入参数初始化完成的 `WsService` (WebSocket 服务) 实例。
    pub fn new(config: WebSocketConfig, connection_manager: Arc<ConnectionManager>) -> Self {
        info!("[WebSocket服务层] 正在创建并初始化一个新的 WsService (WebSocket 服务) 实例...");
        Self {
            config, // 存储传入的配置
            connection_manager, // 存储对连接管理器的共享引用
        }
    }

    /// 异步启动 WebSocket 服务端并开始监听连接。
    ///
    /// 此方法是 `WsService` (WebSocket 服务) 的核心入口点。一旦调用，它将：
    /// 1.  记录启动信息和配置详情。
    /// 2.  调用 `rust_websocket_utils::server::transport::start_server` (启动服务器) 函数来启动底层的
    ///     WebSocket 服务器。此函数需要一个监听地址和一个回调闭包 (`on_new_connection_cb` - 新连接回调)。
    /// 3.  `start_server` (启动服务器) 函数会在后台开始监听指定的网络地址和端口。当有新的客户端
    ///     尝试建立 WebSocket 连接时，它会接受连接，然后为这个新连接执行提供的
    ///     `on_new_connection_cb` (新连接回调) 回调闭包。
    /// 4.  这个回调闭包 (`on_new_connection_cb` - 新连接回调) 负责为每个新连接设置完整的处理逻辑，
    ///     包括创建 `ClientSession` (客户端会话)、注册到 `ConnectionManager` (连接管理器)、以及派生两个并发的
    ///     Tokio 任务分别用于处理消息的发送和接收/路由 (详见模块级文档和闭包内部注释)。
    ///
    /// # 返回值
    /// * `Result<(), anyhow::Error>`: 
    ///   - 如果服务器成功启动并开始监听，此函数本身不会立即返回（因为它通常是启动一个后台服务）。
    ///     `start_server` (启动服务器) 内部可能会阻塞或持续运行。
    ///   - 如果在尝试启动服务器时发生错误 (例如，端口已被占用、网络配置问题等)，
    ///     则会返回一个包含错误详情的 `Err(anyhow::Error)`。
    ///   - 如果服务器在运行过程中遇到无法恢复的严重错误导致其意外终止，`start_server` (启动服务器)
    ///     函数也可能返回错误。
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        info!("[WebSocket服务层] WebSocket 服务正在启动...");
        info!(
            "[WebSocket服务层] 当前服务配置详情：将监听主机地址 '{}', 监听端口号 {}",
            self.config.host, self.config.port
        );

        // 注意：`ConnectionManager` (连接管理器) (以及未来可能有的 `TaskStateManager` - 任务状态管理器) 的实例是在 `main.rs` (主程序入口) 的 `setup` (设置) 钩子中
        // 创建并初始化为 `tauri::State` (Tauri 状态) 的。本 `WsService` (WebSocket 服务) 实例在创建时通过构造函数接收了对
        // `ConnectionManager` (连接管理器) 的 `Arc` (原子引用计数) 共享引用，因此此处直接使用 `self.connection_manager` 即可。

        // 定义当 `rust_websocket_utils::start_server` (启动服务器) 接受一个新的客户端 WebSocket 连接时要执行的回调闭包。
        // 这个闭包是异步的 (`async move`)，并且对于每一个成功建立的新连接，都会在其自己的 Tokio 任务中执行。
        let on_new_connection_cb = {
            // 为闭包克隆 `Arc<ConnectionManager>`。因为闭包会捕获其环境，并且可能比当前函数活得更久
            // (特别是当它被传递给另一个异步任务时)，我们需要确保它拥有对 `ConnectionManager` (连接管理器) 的有效共享引用。
            // `Arc::clone` 只会增加引用计数，不会深拷贝数据。
            let conn_manager_for_cb = Arc::clone(&self.connection_manager);
            
            // `move` 关键字确保闭包捕获其使用的外部变量 (如 `conn_manager_for_cb`) 的所有权 (对于 `Arc` 来说是克隆的引用)。
            // 此闭包接收两个参数，均由 `rust_websocket_utils::start_server` (启动服务器) 在新连接建立时提供：
            // - `ws_conn_handler`: 一个 `WsConnectionHandler` (WebSocket 连接处理器) 实例，封装了向此特定客户端连接发送消息的方法。
            // - `ws_receiver`: 一个 `SplitStream<WebSocketStream<TcpStream>>` (分离的WebSocket TCP流)，代表了从此客户端连接接收消息的流。
            move |ws_conn_handler: WsConnectionHandler, mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>| {
                // 再次为派生的 `async` 块克隆 `Arc<ConnectionManager>`。
                // 这是因为每个新连接都会在一个独立的 `async` 块 (通常是一个新的 Tokio 任务) 中处理，
                // 这个 `async` 块也需要对 `ConnectionManager` (连接管理器) 的共享所有权。
                let connection_manager_clone_for_async_block = Arc::clone(&conn_manager_for_cb);
                
                // 为每个新接受的客户端连接创建一个新的异步任务。
                // 这个任务将负责该连接的整个生命周期内的消息收发和处理。
                async move {
                    // 创建一个 MPSC (多生产者单消费者) 通道，用于从其他业务逻辑模块 (如 `MessageRouter` - 消息路由器) 
                    // 向此客户端的专属发送任务传递待发送的 `ActualWsMessage` (实际 WebSocket 消息)。
                    // 通道缓冲区大小设置为 32条消息。如果发送速度超过处理速度导致缓冲区满，则 `send` (发送) 操作会异步等待。
                    let (tx_to_client_session, mut rx_from_client_session) = mpsc::channel::<ActualWsMessage>(32);
                    
                    // --- 与 `rust_websocket_utils` (公司内部 WebSocket 工具库) 集成时的注意点与临时解决方案 (P3.1.1 问题记录) ---
                    // 中文详细说明：以下代码段涉及到与 `rust_websocket_utils` 库当前实现的一些已知限制和相应的临时处理策略。
                    // 理想情况下，客户端的真实网络地址和连接关闭控制句柄应由该库直接提供。
                    // 在库功能更新或完善之前，我们采取了如下的占位符和本地管理机制。

                    // 问题1: 获取客户端真实网络地址 (P3.1.1 问题记录)
                    // 中文说明：`rust_websocket_utils` 提供的 `WsConnectionHandler` (WebSocket 连接处理器) 当前版本
                    // 可能未能直接暴露已连接客户端的实际 `SocketAddr` (套接字地址)。
                    // 期望的理想情况是: let actual_addr = ws_conn_handler.addr(); // 假设 `WsConnectionHandler` 有这样一个方法。
                    // 当前临时方案：在库提供此功能之前，我们暂时使用一个固定的、无效的占位符地址 "0.0.0.0:0"。
                    // 这会导致在日志记录和 `ClientSession` (客户端会话) 中存储的客户端地址并非其实际来源，
                    // 可能会给问题追踪和调试带来不便。等待 `rust_websocket_utils` 库的后续更新来解决此问题。
                    let actual_addr: std::net::SocketAddr = "0.0.0.0:0".parse().expect("内部严重错误：无法解析占位符 SocketAddr (套接字地址)！请检查代码。");
                    warn!(
                        "[WebSocket服务层] 新客户端连接 (其会话ID稍后分配) **重要注意**：当前为此连接使用了固定的占位符网络地址 '{}'。"
                        + "这并非客户端的真实IP和端口。请尽快检查或推动 `rust_websocket_utils` 库的更新，以确保 `WsConnectionHandler` (WebSocket 连接处理器) 能够正确提供并传递真实的客户端网络地址。",
                        actual_addr
                    );

                    // 问题2: 获取物理连接的关闭句柄 (P3.1.1 问题记录)
                    // 中文说明：`rust_websocket_utils` 的 `WsConnectionHandler` (WebSocket 连接处理器) 当前版本可能没有提供一个
                    // 由其管理的、允许从外部（例如本 `WsService` 模块）主动请求关闭此物理 WebSocket 连接的句柄或机制。
                    // 期望的理想情况是: let close_handle = ws_conn_handler.get_close_handle(); // 假设 `WsConnectionHandler` 提供此类关闭句柄。
                    // 当前临时方案：我们在本地为每个连接创建一个 `Arc<AtomicBool>` (原子布尔类型的共享引用) 作为逻辑上的"关闭标志" (`close_handle`)。
                    // 当 `ConnectionManager::remove_client` (移除客户端) (例如因心跳超时或业务逻辑请求) 尝试通过设置此本地 `close_handle` 
                    // 为 `true` 来"关闭"连接时，它实际上仅能通知本模块内部为此连接派生的发送任务和接收循环终止其各自的活动。
                    // 物理的 TCP/WebSocket 连接的实际关闭，将依赖于 `WsConnectionHandler` (WebSocket 连接处理器) 和 `SplitStream` (分离的流) 在其 `drop` (销毁) 
                    // 时是否能够正确地关闭底层的网络资源，或者依赖于 `receive_message` (接收消息) 函数在侦测到对端关闭连接时能否正确返回相应的错误指示。
                    // 如果 `rust_websocket_utils` 不保证这些行为，则服务器可能无法主动、立即地断开物理链路，
                    // 仅能停止处理该连接上的进一步消息，并依赖于更底层的超时或错误来最终回收资源。
                    let close_handle = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    warn!(
                        "[WebSocket服务层] 新客户端连接 (其会话ID稍后分配，占位符地址: {}) **重要注意**：当前为该连接创建了一个本地的逻辑关闭标志 (`Arc<AtomicBool>`)。"
                        + "通过此标志从 `ConnectionManager` (连接管理器) 发起的连接关闭请求，其主要作用是通知本模块内部的消息收发任务终止。"
                        + "物理连接的实际和及时关闭，依赖于 `rust_websocket_utils` 库中相关组件 (`WsConnectionHandler`, `SplitStream`) 的 `Drop` (销毁) 实现的健壮性。"
                        + "为了确保服务器能够更可控、更主动地管理物理连接的关闭，强烈建议增强 `rust_websocket_utils` 库，使其提供自身的、明确的物理连接关闭句柄或机制。",
                        actual_addr
                    );

                    // 调用 `ConnectionManager::add_client` (添加客户端) 方法来：
                    // 1. 创建一个新的 `ClientSession` (客户端会话) 实例。
                    // 2. 为该会话生成一个唯一的 `client_id` (客户端ID，通常是UUID)。
                    // 3. 将此 `ClientSession` (客户端会话) 注册到 `ConnectionManager` (连接管理器) 的内部状态中 (例如，一个并发安全的哈希映射)。
                    // `add_client` (添加客户端) 需要客户端的地址 (当前是占位符)、用于向客户端发送消息的 MPSC 通道的发送端 (`tx_to_client_session`)，
                    // 以及我们本地创建的、用于逻辑上请求关闭此会话相关任务的原子布尔标志 (`close_handle`)。
                    let client_session = connection_manager_clone_for_async_block.add_client(
                        actual_addr,             // 客户端网络地址 (当前为占位符，见P3.1.1问题记录)。
                        tx_to_client_session,    // MPSC 通道的发送端。`ClientSession` (客户端会话) 将持有此发送端，以便其他模块可以将消息路由给它。
                        close_handle             // 本地创建的、用于从外部请求关闭此会话相关任务的原子布尔标志 (逻辑关闭信号)。
                    ).await; // `add_client` (添加客户端) 是一个异步方法。
                    
                    info!(
                        "[WebSocket服务层] 新客户端已成功连接并注册到 ConnectionManager (连接管理器)。分配的会话ID: {}, 使用的(占位符)地址: {:?}. "
                        + "现在将为此客户端连接启动并发的消息发送和接收任务。",
                        client_session.client_id,
                        client_session.addr // 再次提醒：此地址当前是占位符，并非真实客户端地址
                    );

                    // 为此客户端连接分别设置并派生两个独立的、并发的异步任务：一个用于发送消息，另一个用于接收和处理消息。
                    
                    // --- 消息发送任务 (`sender_task`) ---
                    // 此任务的主要职责是：
                    // - 监听 `rx_from_client_session` (MPSC 通道的接收端)。
                    // - 当从通道接收到 `ActualWsMessage` (实际 WebSocket 消息) 时，使用 `ws_conn_handler` (来自 `rust_websocket_utils` 的 WebSocket 连接处理器) 
                    //   将该消息异步发送到物理的 WebSocket 连接。
                    // - 定期检查 `client_session.connection_should_close` (连接应关闭) 标志，并在标志被置位 (设置为 true) 或 MPSC 通道关闭时终止自身。
                    let client_session_id_for_sender_task = client_session.client_id; // 复制 client_id (客户端ID) 用于日志记录，避免在异步闭包中重复访问Arc内部
                    // 将 `ws_conn_handler` (用于发送消息到物理连接的处理器) 的所有权转移给这个新的发送任务。
                    // 当这个发送任务结束时，其拥有的 `sender_task_ws_conn_handler_owner` 会被 `drop` (销毁)。
                    let sender_task_ws_conn_handler_owner = ws_conn_handler; 
                    // 为发送任务克隆对 `ClientSession` (客户端会话) 的共享引用 (`Arc`)。
                    let client_session_for_sender_task = Arc::clone(&client_session);

                    let sender_task_join_handle = tokio::spawn(async move { // 使用 tokio::spawn 派生一个新的异步发送任务
                        // 在任务内部，我们从 `sender_task_ws_conn_handler_owner` 取得 `WsConnectionHandler` (WebSocket 连接处理器) 的实际所有权。
                        let mut sender_task_ws_conn_handler = sender_task_ws_conn_handler_owner; 
                        
                        info!("[WebSocket服务层-发送任务 {}] 发送任务已成功启动。正在等待从MPSC内部消息通道接收消息，并准备通过物理连接发送至客户端。", client_session_id_for_sender_task);
                        
                        loop { // 发送循环，将持续运行，直到被明确的条件 (如关闭信号或通道关闭) 中断
                            // 在尝试从MPSC通道接收消息之前，优先检查是否已收到外部的关闭连接请求。
                            // 使用 `Ordering::SeqCst` (顺序一致性内存序) 来确保对 `connection_should_close` 标志的读写在所有线程/任务间具有全局一致的顺序。
                            if client_session_for_sender_task.connection_should_close.load(std::sync::atomic::Ordering::SeqCst) {
                                info!(
                                    "[WebSocket服务层-发送任务 {}] 检测到来自 ClientSession (客户端会话) 的逻辑关闭信号 (connection_should_close 标志为 true)。发送任务即将优雅终止。",
                                    client_session_id_for_sender_task
                                );
                                break; // 收到关闭信号，立即退出发送循环
                            }

                            // 使用 `tokio::select!` 宏来实现一个带超时的、可中断的从 MPSC 通道接收消息的操作。
                            // 这种机制确保了即使MPSC通道长时间内没有新的消息到达，发送任务也能周期性地"醒来"，
                            // 以便有机会检查 `connection_should_close` (连接应关闭) 标志。
                            tokio::select! {
                                biased; // `biased` 关键字指示 `select!` 在有多个分支同时就绪时，优先处理排列在前面的分支。
                                        // 在当前场景下，主要是为了确保当 `rx_from_client_session.recv()` 可能长时间阻塞（等待消息）时，
                                        // `tokio::time::sleep` (短暂休眠) 分支仍能被周期性地评估，从而使得关闭信号的检查得以执行。
                                
                                // 分支1: 添加一个短暂的休眠 (例如100毫秒)，作为周期性唤醒和检查关闭信号的机制。
                                // 这可以避免在 MPSC 通道为空且关闭信号尚未被设置的情况下，`loop` (循环) 产生过于密集的、无意义的 CPU 旋转检查。
                                // 当这个休眠结束时，`select!` 的这个分支完成，然后 `loop` (循环) 会继续到下一次迭代，重新检查关闭信号和MPSC通道。
                                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                                    // 短暂休眠已结束。此处无需执行任何特定操作，外部的 `loop` (循环) 将自然进入下一次迭代，
                                    // 这就保证了 `connection_should_close` (连接应关闭) 标志会被定期检查。
                                    continue; // 跳过本次 `select!` 的其余分支，直接开始下一次 `loop` (循环) 迭代。
                                }
                                
                                // 分支2: 尝试从 `rx_from_client_session` (MPSC 通道的接收端) 异步接收下一条待发送的消息。
                                // 如果 MPSC 通道已被其所有发送端关闭 (dropped)，则 `recv()` 会立即返回 `None`。
                                maybe_msg_to_send = rx_from_client_session.recv() => {
                                    if let Some(ws_msg_to_send) = maybe_msg_to_send { // 如果成功从MPSC通道接收到一条消息...
                                        debug!(
                                            "[WebSocket服务层-发送任务 {}] 从MPSC内部消息通道成功接收到一条消息，准备通过物理WebSocket连接发送给客户端。消息类型: '{}'",
                                            client_session_id_for_sender_task, ws_msg_to_send.message_type
                                        );
                                        // 尝试使用 `WsConnectionHandler::send_message` (发送消息) 将此消息异步发送到实际的客户端。
                                        // 这是一个异步操作，需要 `.await`。
                                        if sender_task_ws_conn_handler.send_message(&ws_msg_to_send).await.is_err() {
                                            // 如果 `send_message` (发送消息) 返回错误 (例如，底层的 WebSocket 连接已损坏、被对端关闭，或发生其他IO错误)...
                                            error!(
                                                "[WebSocket服务层-发送任务 {}] 通过 WsConnectionHandler (WebSocket 连接处理器) 向客户端发送消息时失败。这通常表示底层物理连接已损坏或已被对端关闭。发送任务将因此终止。",
                                                client_session_id_for_sender_task
                                            );
                                            // 可选操作：如果发送失败，也可以在这里主动设置 `ClientSession` 的逻辑关闭标志，
                                            // 尽管通常情况下，如果物理连接出问题，接收端 (`receiver_loop`) 应该会更早或同时检测到，并设置此标志。
                                            // client_session_for_sender_task.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst);
                                            break; // 既然无法发送，退出发送循环
                                        }
                                        // 如果消息发送成功，则记录调试信息，并继续下一次 `loop` (循环) 迭代，等待更多消息。
                                        debug!(
                                            "[WebSocket服务层-发送任务 {}] 消息 (类型: '{}') 已通过 WsConnectionHandler (WebSocket 连接处理器) 成功提交给发送队列或已发送。",
                                            client_session_id_for_sender_task, ws_msg_to_send.message_type
                                        );
                                    } else { // 如果 `rx_from_client_session.recv()` 返回 `None`...
                                             // 这通常意味着 MPSC 通道的所有发送端 (`tx_to_client_session` 及其所有克隆) 都已被 `drop` (销毁)。
                                             // 这种情况的典型原因是持有 `tx_to_client_session` 的 `ClientSession` (客户端会话) 实例自身
                                             // 已被从 `ConnectionManager` (连接管理器) 中移除并最终被销毁。
                                             // 这表明，逻辑上已没有更多的消息会通过此MPSC通道发送给该客户端了。
                                        info!(
                                            "[WebSocket服务层-发送任务 {}] MPSC 内部消息通道 (rx_from_client_session) 已被关闭 (其 recv() 方法返回 None)。这通常意味着关联的 ClientSession (客户端会话) 已被清理，没有更多消息需要发送。发送任务即将优雅终止。",
                                            client_session_id_for_sender_task
                                        );
                                        break; // MPSC通道关闭，退出发送循环
                                    }
                                }
                            } // tokio::select! 宏的结束
                        } // loop (发送循环) 的结束
                        
                        info!("[WebSocket服务层-发送任务 {}] 发送循环已正常退出。发送任务执行完毕，即将完全结束。", client_session_id_for_sender_task);
                        // 当此异步发送任务结束时，其所拥有的 `sender_task_ws_conn_handler` (即 `WsConnectionHandler` - WebSocket 连接处理器) 会被自动 `drop` (销毁)。
                        // 我们依赖 `rust_websocket_utils` 库确保 `WsConnectionHandler::drop` 的实现能够正确地清理和关闭相关的底层网络连接资源。
                    }); // tokio::spawn (异步发送任务) 的结束
                    
                    // --- 消息接收与处理循环 (`receiver_loop`) ---
                    // 此循环的主要职责是：
                    // - 持续地从 `ws_receiver` (代表 WebSocket 连接流的接收端，由 `rust_websocket_utils` 提供) 尝试接收来自客户端的消息。
                    // - 如果成功接收到消息，将其连同相关的上下文 (如 `ClientSession` 和 `ConnectionManager` 的共享引用)
                    //   传递给 `message_router::handle_message` (消息路由器的处理函数) 进行进一步的路由和业务逻辑处理。
                    // - 此循环也实现了带超时的非阻塞接收，以便能够周期性地检查 `client_session.connection_should_close` (连接应关闭) 标志。
                    // - 在检测到逻辑关闭信号被置位、或从 `receive_message` (接收消息) 函数收到连接已实际关闭的指示、或发生不可恢复的协议错误时，
                    //   此循环会优雅地终止自身。
                    let client_session_clone_for_router = Arc::clone(&client_session); // 为接收循环内部及传递给消息路由器克隆对 ClientSession (客户端会话) 的共享引用。
                    let connection_manager_for_router = Arc::clone(&connection_manager_clone_for_async_block); // 为消息路由器克隆对 ConnectionManager (连接管理器) 的共享引用。
                    // 注意：`ws_receiver` (SplitStream - 分离的流) 的所有权被完整地移入此即将开始的接收循环中。
                    info!("[WebSocket服务层-接收循环 {}] 接收与处理循环已成功启动。正在等待从客户端通过物理WebSocket连接接收消息。", client_session_clone_for_router.client_id);
                    loop { // 接收与处理循环，将持续运行，直到被明确的条件 (如关闭信号或连接错误) 中断
                        // 在尝试从网络连接接收新消息之前，优先检查是否已收到外部的逻辑关闭连接请求。
                        if client_session_clone_for_router.connection_should_close.load(std::sync::atomic::Ordering::SeqCst) {
                            info!(
                                "[WebSocket服务层-接收循环 {}] 检测到来自 ClientSession (客户端会话) 的逻辑关闭信号 (connection_should_close 标志为 true)。接收与处理循环即将优雅终止。",
                                client_session_clone_for_router.client_id
                            );
                            break; // 收到关闭信号，立即退出接收循环
                        }

                        // 使用 `tokio::select!` 宏配合一个超时机制来实现非阻塞的、可中断的消息接收尝试。
                        // 这种模式使得即使网络上长时间没有新的消息到达，此接收循环也能周期性地"醒来"，
                        // 从而有机会检查 `connection_should_close` (连接应关闭) 逻辑标志。
                        // 注意: 对 `receive_message(&mut ws_receiver)` 的调用被包装在 `Box::pin(...)` 中。
                        // 这是因为 `tokio::select!` 要求其监听的所有 future (异步操作) 都必须是 `Unpin` (可以安全地在内存中移动的)。
                        // 而直接由异步函数调用产生的 future 可能不是 `Unpin` 的 (特别是如果它们内部有自引用)。
                        // `Box::pin` 将这个 future "固定"到堆内存上，从而使其满足 `Unpin` 的约束。
                        let mut receive_fut = Box::pin(receive_message(&mut ws_receiver)); 
                        let received_result = tokio::select! {
                            biased; // `biased` 关键字指示 `select!` 在有多个分支同时就绪时，优先处理排列在前面的分支。
                                    // 这里主要是为了确保 `receive_fut` (实际的消息接收操作) 能够被优先尝试。
                            
                            // 分支1: 监听 `receive_message` 异步函数的结果。
                            // 当 `receive_message` 完成时 (无论是成功接收到消息还是发生错误)，`res` 将会是 `Result<ActualWsMessage, WsError>`。
                            // 我们将这个 `Result` 包装在 `Some` 里面，以便与下面的超时情况 (超时分支返回 `None`) 进行区分。
                            res = &mut receive_fut => Some(res), 
                            
                            // 分支2: 监听一个1秒钟的超时。如果在这1秒钟内，上面的 `receive_fut` (消息接收操作) 没有完成，
                            // 那么这个 `tokio::time::sleep` (休眠) 分支就会被选中，并导致 `received_result` 被赋值为 `None`。
                            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => None, 
                        };

                        match received_result { // `received_result` 的类型是 `Option<Result<ActualWsMessage, WsError>>`
                            Some(Ok(ws_msg)) => { // 情况 1: `Some(Ok(msg))` 表示：在超时前成功接收并解析了一个有效的 `ActualWsMessage` (实际 WebSocket 消息)。
                                // 将接收到的消息以及相关的上下文信息 (客户端会话、连接管理器) 传递给消息路由器进行后续处理。
                                // `message_router::handle_message` (消息路由器的处理函数) 本身也是一个异步函数。
                                if let Err(e) = message_router::handle_message(
                                    Arc::clone(&client_session_clone_for_router), // 传递对 ClientSession (客户端会话) 的共享引用
                                    ws_msg,                                       // 传递刚接收到的 ActualWsMessage (实际 WebSocket 消息)
                                    Arc::clone(&connection_manager_for_router),   // 传递对 ConnectionManager (连接管理器) 的共享引用
                                    // TODO (P3.3.2 任务状态同步): 当实现任务状态管理时，可能需要在此处取消注释并传递对 TaskStateManager (任务状态管理器) 的共享引用。
                                    // Arc::clone(&task_state_manager_for_router) 
                                )
                                .await // 等待消息路由器处理此消息完成
                                {
                                    // 如果 `handle_message` (消息处理函数) 返回了一个错误 (虽然它通常被设计为在内部处理大多数可恢复的子错误并返回 `Ok(())`)... 
                                    // 这种错误可能表示消息处理过程中发生了某种未被内部捕获的、更严重的问题。
                                    error!(
                                        "[WebSocket服务层-接收循环 {}] 消息路由器 (message_router::handle_message) 在处理来自客户端的消息时报告了一个未处理的错误: {}. 接收循环将继续尝试处理后续消息。",
                                        client_session_clone_for_router.client_id, e
                                    );
                                    // 根据错误的具体性质，可能需要进一步决策是否应该中断整个接收循环。
                                    // 当前的策略是：仅记录此错误，并假定错误是消息级别的，然后继续尝试接收和处理下一条消息。
                                    // 如果错误确实是灾难性的，那么 `handle_message` (消息处理函数) 自身或者其调用的业务逻辑应该考虑如何影响服务或连接的整体状态 (例如，通过设置关闭标志)。
                                }
                            }
                            Some(Err(ws_err)) => { // 情况 2: `Some(Err(err))` 表示：在超时前，`receive_message` (接收消息) 函数在尝试从 `ws_receiver` 接收或解析消息时发生了 `WsError` (WebSocket错误)。
                                // 我们需要根据 `rust_websocket_utils` 库返回的 `WsError` (WebSocket错误) 的具体类型来决定如何响应。
                                match ws_err {
                                    WsError::DeserializationError(e) => { // 子情况 2.1: 如果错误是消息负载反序列化失败...
                                        warn!(
                                            "[WebSocket服务层-接收循环 {}] 从客户端接收到的某条消息，在 `rust_websocket_utils::receive_message` (接收消息) 内部阶段发生负载反序列化错误: {}. "
                                            + "这通常意味着客户端发送了格式不正确或类型不匹配的JSON负载。消息路由器 (`message_router`) 通常会尝试向客户端发送具体的错误响应。接收循环将继续。",
                                            client_session_clone_for_router.client_id, e
                                        );
                                        // 重要提示：单个消息的负载反序列化失败通常不应该导致整个 WebSocket 连接被中断。
                                        // `message_router::handle_message` (消息路由器的处理函数) 在其内部逻辑中，对于解析到的、已知类型的消息，如果其负载进一步解析失败，
                                        // 应该已经负责向客户端发送具体的错误响应消息（例如 `ErrorResponsePayload` 或特定类型的失败响应）。
                                        // 此处记录的 `WsError::DeserializationError` (反序列化错误) 更可能是指 `receive_message` (接收消息) 函数在尝试将原始字节流转换为 `ActualWsMessage` 结构体
                                        // (包含 `message_type` 和 `payload` 字符串) 这一更早阶段发生的错误，例如整个消息结构本身不符合预期。
                                        // 即使如此，我们也倾向于不立即断开连接，而是依赖消息路由器后续处理（如果消息能被部分解析并传递给它的话），
                                        // 或者如果错误严重到无法构造出 `ActualWsMessage`，则可能需要重新审视 `receive_message` 的行为或错误报告机制。
                                        // 当前，我们仅记录警告并继续循环。
                                    }
                                    WsError::WebSocketProtocolError(e) => { // 子情况 2.2: 如果是 WebSocket 协议级别的错误 (例如，无效的帧序列、不符合协议的握手后行为等)...
                                        warn!(
                                            "[WebSocket服务层-接收循环 {}] 检测到严重的 WebSocket 协议级错误: {}. "
                                            + "这通常表示客户端行为异常，或者网络连接已严重损坏。接收与处理循环即将因此终止。",
                                            client_session_clone_for_router.client_id, e
                                        );
                                        client_session_clone_for_router.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst); // 主动设置逻辑关闭标志
                                        break; // WebSocket 协议错误通常被认为是不可恢复的，应立即终止此连接的处理。
                                    }
                                    WsError::ConnectionClosed => { // 子情况 2.3: 如果 `rust_websocket_utils::receive_message` (接收消息) 明确返回 `ConnectionClosed` (连接已关闭) 错误...
                                        info!(
                                            "[WebSocket服务层-接收循环 {}] 底层 `receive_message` (接收消息) 函数报告连接已被对端关闭 (返回 WsError::ConnectionClosed)。接收与处理循环即将因此终止。",
                                            client_session_clone_for_router.client_id
                                        );
                                        client_session_clone_for_router.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst); // 确保逻辑关闭标志也被设置
                                        break; // 连接既然已被关闭，自然应终止接收循环。
                                    }
                                    WsError::Message(s) => { // 子情况 2.4: 如果是 `rust_websocket_utils` 内部定义的、通过字符串消息传递的通用错误...
                                        warn!(
                                            "[WebSocket服务层-接收循环 {}] 底层 `receive_message` (接收消息) 函数报告了一个内部消息错误: '{}'. "
                                            + "由于此类错误的具体性质未知，我们将根据其潜在的严重性，假定连接可能存在问题，并终止接收与处理循环。",
                                            client_session_clone_for_router.client_id, s
                                        );
                                        client_session_clone_for_router.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst); // 主动设置逻辑关闭标志
                                         break; // 鉴于错误的来源和性质不够明确，保守地假定这类内部消息错误是严重的，并终止循环。
                                    }
                                    // 需要检查 `rust_websocket_utils::error::WsError` 的完整定义，以确保所有可能的错误变体都得到妥善处理。
                                    // 例如，常见的还可能包括：
                                    // - `WsError::IoError(std::io::Error)`: 底层 TCP/IP 套接字发生IO错误。
                                    // - `WsError::Tungstenite(tokio_tungstenite::tungstenite::Error)`: 来自底层 `tokio-tungstenite` 库的特定错误。
                                    // 对于这些类型的错误，通常也应被认为是连接级的问题，并应导致接收循环的终止。
                                    other_ws_err => { // 子情况 2.5: 捕获所有其他未被前面分支明确处理的 `WsError` (WebSocket错误) 类型。
                                        error!(
                                            "[WebSocket服务层-接收循环 {}] 从底层 `receive_message` (接收消息) 函数收到了一个当前未被明确处理的 WsError (WebSocket错误) 类型: {:?}. "
                                            + "为确保系统稳定性并避免未知行为，我们将采取保守策略，假定连接存在严重问题，并因此终止接收与处理循环。",
                                            client_session_clone_for_router.client_id, other_ws_err
                                        );
                                        client_session_clone_for_router.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst); // 主动设置逻辑关闭标志
                                        break; // 对于任何未知或未明确分类处理的 `WsError` (WebSocket错误)，最安全的做法是断开连接并终止循环。
                                    }
                                }
                            }
                            None => { // 情况 3: `None` 表示 `tokio::select!` 中的 `tokio::time::sleep` (休眠) 分支被选中，即消息接收在一秒内超时。
                                // 接收超时本身并不是一个错误。这仅仅表示在设定的超时期限内，客户端没有发送任何新的消息。
                                // 接收循环将继续运行，以便在下一次迭代中再次检查逻辑关闭信号并重新尝试从网络接收消息。
                                debug!(
                                    "[WebSocket服务层-接收循环 {}] 在尝试从客户端WebSocket连接接收新消息时发生了一次短暂的超时 (1秒)。将继续监听后续消息。",
                                    client_session_clone_for_router.client_id
                                );
                                // 此处无需执行 `break` 或 `continue`；`loop` (循环) 将自然地进入下一次迭代。
                            }
                        } // match received_result (匹配接收结果) 的结束
                    } // loop (接收与处理循环) 的结束

                    info!(
                        "[WebSocket服务层-接收循环 {}] 接收与处理循环已正常退出。",
                        client_session_clone_for_router.client_id
                    );
                    
                    // 当接收与处理循环结束后 (这通常意味着物理连接已关闭，或者逻辑上被请求关闭，或者发生了不可恢复的错误)，
                    // 我们也应该确保并发的发送任务 (`sender_task`) 被及时通知并终止。
                    // 设置 `client_session.connection_should_close` 标志是主要的通知机制，确保发送任务在下次检查时会退出。
                    // 即使接收循环是因为此标志已被设置为 true 而退出的，再次设置也无害。
                    client_session.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst);
                    info!(
                        "[WebSocket服务层-连接处理 {}] 客户端 {} 的接收循环已结束。已再次确保其 ClientSession (客户端会话) 的 connection_should_close (连接应关闭) 逻辑标志被设置为 true，以便通知其对应的发送任务也应尽快终止。",
                        client_session.client_id, client_session.client_id // 重复 client_id 以强调是哪个客户端
                    );

                    // 等待对应的发送任务 (`sender_task`) 完成。
                    // 这样做是为了确保在最终从 `ConnectionManager` (连接管理器) 中移除此客户端会话之前，
                    // 所有可能已在MPSC通道中排队等待发送的消息都有机会被尝试发送出去，
                    // 或者至少确保发送任务能够优雅地响应关闭信号并完成其清理工作。
                    if let Err(e) = sender_task_join_handle.await {
                        // 如果 `sender_task_join_handle.await` 返回错误，通常意味着被等待的发送任务发生了 panic (恐慌)。
                        error!(
                            "[WebSocket服务层-连接处理 {}] 在等待客户端 {} 的发送任务 (sender_task) 完成时发生了一个错误 (该任务可能已经 panic 或被异常终止): {:?}.",
                            client_session.client_id, client_session.client_id, e
                        );
                    } else {
                        info!(
                            "[WebSocket服务层-连接处理 {}] 客户端 {} 的发送任务 (sender_task) 已成功结束并被 join (汇合)。",
                            client_session.client_id, client_session.client_id
                        );
                    }
                    
                    // 在此客户端连接的发送和接收任务均已结束后，执行最后的清理步骤：
                    // 从 `ConnectionManager` (连接管理器) 中移除此客户端的会话 (`ClientSession`)。
                    // 这一步至关重要，因为它会：
                    // - 清理与此客户端相关的所有状态信息 (例如，从全局客户端映射中移除)。
                    // - 如果此客户端曾加入某个组，它将被从该组中移除。
                    // - 可能会触发向同组的伙伴客户端发送关于此客户端下线的通知。
                    // - (在P3.3.1与 `TaskStateManager` - 任务状态管理器 - 集成后) 可能还会触发与此客户端或其所在组相关的任务状态的清理或更新。
                    info!(
                        "[WebSocket服务层-连接处理 {}] 客户端 {} 的消息发送和接收任务均已结束或已被通知结束。现在将从 ConnectionManager (连接管理器) 中正式移除此客户端的会话。",
                        client_session.client_id, client_session.client_id
                    );
                    connection_manager_clone_for_async_block.remove_client(&client_session.client_id).await;
                    info!(
                        "[WebSocket服务层-连接处理 {}] 客户端 {} 的会话已成功从 ConnectionManager (连接管理器) 中移除。此客户端连接的完整处理与清理流程至此结束。",
                        client_session.client_id, client_session.client_id
                    );
                    // 当这个 `async move { ... }` 异步块执行完毕并结束时，其所拥有的 `ws_receiver` (SplitStream - 分离的流) 将被自动 `drop` (销毁)。
                    // 我们依赖 `rust_websocket_utils` 库确保 `SplitStream::drop` (或其内部持有的底层WebSocket资源) 的实现能够正确地关闭物理连接。
                } // async move (单个客户端连接的完整处理任务) 的结束
            } // on_new_connection_cb (新连接回调) 闭包定义的结束
        }; // 回调闭包赋值的结束

        // 调用 `rust_websocket_utils::server::transport::start_server` (启动服务器) 函数来实际启动 WebSocket 服务器。
        // 此函数需要一个格式为 "host:port" (例如 "127.0.0.1:8088") 的监听地址字符串，以及我们上面定义的 `on_new_connection_cb` (新连接回调) 回调闭包。
        // `start_server` (启动服务器) 本身是一个异步函数，并且设计上通常会在其内部持续运行 (或阻塞当前任务) 以循环接受新的客户端连接。
        // 因此，调用者通常会在 `.await` 之后主要处理其返回的 `Result`，该 `Result` 主要用于指示启动服务器时是否立即发生了错误 (例如端口被占用)。
        // 如果服务器成功启动，`start_server` 在理论上不会"正常"返回，除非整个服务器被外部信号终止或发生不可恢复的内部错误。
        start_server(
            format!("{}:{}", self.config.host, self.config.port), // 构造监听地址字符串，例如 "127.0.0.1:8088"
            on_new_connection_cb, // 传递我们精心定义的、用于处理每一个新WebSocket连接的回调闭包
        )
        .await // 等待 `start_server` (启动服务器) 的完成 (如上所述，它通常会一直运行，除非出错或服务被终止)
        .with_context(|| { // 使用 `anyhow::Context` 为 `start_server` 可能返回的任何错误添加更具体的上下文信息，便于调试
            format!(
                "[WebSocket服务层] 尝试启动 WebSocket 服务并使其监听于地址 '{}:{}' 的操作最终失败了。",
                self.config.host, self.config.port
            )
        })
    } // start (启动) 方法的结束
} // impl WsService (WebSocket 服务实现) 的结束

</rewritten_file>