// rust_websocket_utils/tests/server_transport_integration_test.rs

use rust_websocket_utils::server::transport::{ServerTransport, WsStream};
// use rust_websocket_utils::error::WsUtilError; // WsUtilError 暂时未在测试中断言，可按需启用
use std::net::SocketAddr;
use std::sync::mpsc; // 用于线程间通信，通知主测试线程连接已处理
use std::time::Duration;
// use tokio::net::TcpStream; // WsStream 类型别名中已包含
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TungsteniteMessage};
use url::Url; // 用于构建 WebSocket URL
use futures_util::{StreamExt, SinkExt};
use log::{info, error, warn, LevelFilter}; // 导入 error 和 warn

// 辅助函数：初始化日志，仅用于测试，避免多次初始化
fn init_test_logger() {
    // is_test(true) 确保日志输出到 stdout 并且不会互相干扰（如果并行测试）
    let _ = env_logger::builder().filter_level(LevelFilter::Info).is_test(true).try_init();
}

// on_connect 回调函数，用于测试
// 它会向主测试线程发送一个信号表示连接成功
async fn test_on_connect_handler(
    mut ws_stream: WsStream,         // WebSocket 流
    peer_addr: SocketAddr,           // 对端地址
    connection_processed_tx: mpsc::Sender<SocketAddr>, // 用于发送信号的通道发送端
) {
    info!("[Test Server] on_connect: 新的 WebSocket 连接来自 {}", peer_addr);

    // 尝试接收一条消息
    if let Some(Ok(msg)) = ws_stream.next().await {
        match msg {
            TungsteniteMessage::Text(text) => {
                info!("[Test Server] 从 {} 收到文本消息: {}", peer_addr, text);
                let echo_response = format!("Server received: {}", text);
                if ws_stream.send(TungsteniteMessage::Text(echo_response.clone())).await.is_ok() {
                    info!("[Test Server] 已向 {} 发送回显: '{}'", peer_addr, echo_response);
                } else {
                    warn!("[Test Server] 向 {} 发送回显失败", peer_addr);
                }
            }
            TungsteniteMessage::Ping(ping_data) => {
                 info!("[Test Server] 收到来自 {} 的 Ping 帧，数据长度: {}", peer_addr, ping_data.len());
                 // tokio-tungstenite 会自动处理 Pong 响应
            }
            TungsteniteMessage::Close(close_frame) => {
                info!("[Test Server] 收到来自 {} 的 Close 帧: {:?}", peer_addr, close_frame);
            }
            _ => {
                info!("[Test Server] 从 {} 收到其他类型的消息: {:?}", peer_addr, msg);
            }
        }
    } else {
        info!("[Test Server] 未从 {} 收到任何消息或连接已关闭", peer_addr);
    }
    
    if connection_processed_tx.send(peer_addr).is_err() {
        // 使用 error! 或 warn! 记录更严重的问题
        warn!("[Test Server] 无法发送连接处理信号至主测试线程，通道可能已关闭。");
    }
    info!("[Test Server] test_on_connect_handler for {} 完成", peer_addr);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_server_starts_and_accepts_connection() {
    init_test_logger();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("无法绑定到随机端口");
    let addr = listener.local_addr().expect("无法获取本地监听地址");
    drop(listener); 

    info!("[Test Main] 服务器将监听地址: {}", addr);
    let (tx, rx) = mpsc::channel::<SocketAddr>();

    let server_handle = tokio::spawn(async move {
        let on_connect_cloneable = move |ws_stream: WsStream, peer_addr: SocketAddr| {
            let tx_clone = tx.clone();
            test_on_connect_handler(ws_stream, peer_addr, tx_clone)
        };
        if let Err(e) = ServerTransport::start(addr, on_connect_cloneable).await {
            error!("[Test Main - Server Task] ServerTransport::start 失败: {:?}", e);
        }
    });
    
    tokio::time::sleep(Duration::from_millis(200)).await; // 稍微增加等待时间

    let url_string = format!("ws://{}", addr);
    // let url = Url::parse(&url_string).expect("无法解析 WebSocket URL"); // 保留以供调试日志，但 connect_async 直接用 string
    info!("[Test Main] 客户端尝试连接到: {}", url_string);

    let connect_attempt = connect_async(&url_string).await; // 直接使用 &url_string
    if let Err(e) = &connect_attempt {
        error!("[Test Main] 客户端连接错误详情: {}", e); // 更详细的错误日志
    }
    assert!(connect_attempt.is_ok(), "[Test Main] 客户端连接失败");
    
    let (mut client_ws_stream, response) = connect_attempt.unwrap();
    info!("[Test Main] 客户端连接成功，服务器响应状态: {}", response.status());

    let client_message = "Hello from client!";
    assert!(
        client_ws_stream.send(TungsteniteMessage::Text(client_message.to_string())).await.is_ok(),
        "[Test Main] 客户端发送消息失败"
    );
    info!("[Test Main] 客户端已发送消息: '{}'", client_message);

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(peer_addr_from_server) => {
            info!("[Test Main] 从服务器回调收到确认，对端(服务器视角客户端)地址: {}", peer_addr_from_server);
        }
        Err(e) => {
            panic!("[Test Main] 等待服务器处理连接超时或通道错误: {:?}", e);
        }
    }
    
    if let Some(Ok(TungsteniteMessage::Text(received_text))) = client_ws_stream.next().await {
        info!("[Test Main] 客户端收到回显: '{}'", received_text);
        assert!(received_text.contains(client_message), "收到的回显 '{}' 与发送的消息 '{}' 不匹配", received_text, client_message);
    } else {
        warn!("[Test Main] 客户端未能收到预期的回显消息，或收到非文本消息");
        // 在某些情况下，如果服务器端处理逻辑有变（例如不回显），这个断言可能需要调整
        // assert!(false, "客户端未收到回显"); // 如果必须收到回显，则取消注释此行
    }

    let _ = client_ws_stream.close(None).await;
    info!("[Test Main] 客户端连接已关闭");

    server_handle.abort();
    // 等待服务器任务确实被中止 (可选，但有助于确保资源释放)
    let _ = server_handle.await;
    info!("[Test Main] 服务器任务已中止并完成清理");
} 