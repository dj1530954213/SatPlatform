// rust_websocket_utils/src/server/transport.rs

//! 包含服务端 WebSocket 监听、接受连接和通信逻辑。

use crate::error::WsUtilError; // 引入自定义错误类型
use log::{info, error, warn}; // 日志记录 (规则 1.2)
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, WebSocketStream}; // tokio-tungstenite 相关 (规则 1.2)

/// `WsStream` 是一个类型别名，代表经过 WebSocket 握手后的 TCP 流。
pub type WsStream = WebSocketStream<TcpStream>;

/// `ServerTransport` 结构体负责处理 WebSocket 服务端的监听和连接接受。
pub struct ServerTransport;

impl ServerTransport {
    /// 启动 WebSocket 服务器并开始监听指定的地址。
    ///
    /// 对于每一个成功建立的 WebSocket 连接，都会调用 `on_connect` 回调函数进行处理。
    /// 这个服务器会持续运行，直到发生不可恢复的错误 (例如 TCP 监听器绑定失败) 或进程被终止。
    ///
    /// # Arguments
    /// * `addr`: 服务器监听的 `SocketAddr` (例如 "127.0.0.1:8080")。
    /// * `on_connect`: 一个回调函数，当新的 WebSocket 连接建立时被调用。
    ///   该函数接收两个参数：
    ///     - `ws_stream`: 建立的 `WsStream`。
    ///     - `peer_addr`: 连接方的 `SocketAddr`。
    ///   此回调函数必须是 `async` 的，并且是 `Send + Sync + Clone + 'static`，
    ///   因为它会在一个新的 Tokio 任务中为每个连接执行。
    ///
    /// # Returns
    /// * `Result<(), WsUtilError>`: 如果监听器启动失败，则返回错误；否则，此函数将无限期运行。
    ///
    /// # Panics
    /// 此函数本身不应该 panic，错误会通过 `Result` 返回或被日志记录。
    pub async fn start<F, Fut>(addr: SocketAddr, on_connect: F) -> Result<(), WsUtilError>
    where
        F: Fn(WsStream, SocketAddr) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        // 尝试绑定 TCP 监听器到指定地址 (规则 6.1: 使用 tokio 的 async)
        let listener = TcpListener::bind(&addr).await.map_err(WsUtilError::TcpBindError)?;
        info!("WebSocket 服务器正在监听地址: {}", addr); // 日志记录 (规则 5.1)

        // 无限循环以接受新的连接
        loop {
            match listener.accept().await {
                Ok((tcp_stream, peer_addr)) => {
                    info!("从 {} 接受了新的 TCP 连接", peer_addr);
                    
                    // 为每个连接克隆回调函数
                    let on_connect_callback = on_connect.clone();

                    // 为每个连接创建一个新的 Tokio 任务来处理 WebSocket 握手和后续逻辑 (规则 6.1)
                    tokio::spawn(async move {
                        // 执行 WebSocket 握手
                        match accept_async(tcp_stream).await {
                            Ok(ws_stream) => {
                                info!("与 {} 的 WebSocket 握手成功", peer_addr);
                                // 调用用户提供的连接处理回调
                                on_connect_callback(ws_stream, peer_addr).await;
                            }
                            Err(e) => {
                                // 握手失败，记录错误 (规则 5.1)
                                error!("与 {} 的 WebSocket 握手失败: {}", peer_addr, e);
                                // 根据项目规则，这里可以将 WsError 转换为 WsUtilError::HandshakeError，
                                // 但由于是在 spawn 的任务中，直接返回错误比较复杂，通常是记录并终止此特定连接的任务。
                            }
                        }
                    });
                }
                Err(e) => {
                    // 接受 TCP 连接失败，记录错误并继续监听其他连接 (规则 5.1)
                    error!("接受 TCP 连接失败: {}。服务器将继续运行。", e);
                    // 可以考虑是否需要将此错误包装为 WsUtilError::TcpAcceptError 并做进一步处理，
                    // 但对于持续运行的服务器，通常是记录并尝试恢复/继续。
                }
            }
        }
        // 由于是无限循环，正常情况下不会执行到这里。
        // 若要实现优雅关闭，需要额外的逻辑 (例如 select! 配合一个关闭信号)。
        // 对于 P0.3.2，无限循环监听符合要求。
    }
}

// 移除之前的占位注释
// // 暂时为空，后续会根据开发步骤 P0.3.2 添加具体实现。 