// SatCloudService/src-tauri/src/ws_server/heartbeat_monitor.rs

//! 心跳监视器，用于检测和处理超时的客户端连接。

use crate::ws_server::connection_manager::ConnectionManager;
// use crate::ws_server::client_session::ClientSession; // 移除未使用的导入
// use crate::config::WebSocketConfig; // 暂时不直接从这里读取，而是通过构造函数传入超时配置
use chrono::Utc; // 引入 Utc 用于获取当前时间
use log::{info, warn, debug};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// 心跳监视器负责定期检查客户端的活动状态。
// #[derive(Debug)] // ConnectionManager 可能没有 Debug，暂时注释
pub struct HeartbeatMonitor {
    /// 对连接管理器的共享引用，用于访问客户端列表和移除客户端。
    connection_manager: Arc<ConnectionManager>,
    /// 客户端不活动状态的超时时间。
    client_timeout_duration: Duration,
    /// 检查客户端超时的周期性间隔。
    check_interval: Duration,
}

impl HeartbeatMonitor {
    /// 创建一个新的 `HeartbeatMonitor` 实例。
    ///
    /// # Arguments
    ///
    /// * `connection_manager` - 对 `ConnectionManager` 的共享引用。
    /// * `client_timeout_duration` - 客户端允许的最大不活动时间。
    /// * `check_interval` - 执行超时检查的频率。
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        client_timeout_duration: Duration,
        check_interval: Duration,
    ) -> Self {
        info!(
            "[HeartbeatMonitor] New instance created with timeout: {:?}, check_interval: {:?}",
            client_timeout_duration,
            check_interval
        );
        Self {
            connection_manager,
            client_timeout_duration,
            check_interval,
        }
    }

    /// 启动心跳监视器的主循环。
    /// 此方法会持续运行，定期检查超时的客户端。
    pub async fn run(self) {
        info!("[HeartbeatMonitor] Starting run loop...");
        loop {
            sleep(self.check_interval).await;
            debug!("[HeartbeatMonitor] Performing periodic check for timed out clients...");
            self.check_for_timed_out_clients().await;
        }
    }

    /// 检查并处理超时的客户端。
    async fn check_for_timed_out_clients(&self) {
        let clients_snapshot = self.connection_manager.get_all_client_sessions();

        if clients_snapshot.is_empty() {
            debug!("[HeartbeatMonitor] No active clients to check.");
            return;
        }

        debug!("[HeartbeatMonitor] Checking {} active clients for timeout.", clients_snapshot.len());

        let now = Utc::now();

        for client_session in clients_snapshot {
            let client_id = client_session.client_id;
            let last_seen_dt = *client_session.last_seen.read().await;
            
            // 将 std::time::Duration 转换为 chrono::Duration
            // 注意：chrono::Duration::from_std 对于负数或过大的 Duration 会返回错误
            // 但我们的 client_timeout_duration 应该是正数且在合理范围内。
            let chrono_timeout = match chrono::Duration::from_std(self.client_timeout_duration) {
                Ok(duration) => duration,
                Err(e) => {
                    warn!(
                        "[HeartbeatMonitor] Failed to convert client_timeout_duration ({:?}) to chrono::Duration: {}. Using a default of 60s for this check.", 
                        self.client_timeout_duration, e
                    );
                    chrono::Duration::seconds(60) // 提供一个备用值
                }
            };

            if now.signed_duration_since(last_seen_dt) > chrono_timeout {
                warn!(
                    "[HeartbeatMonitor] Client {} (ID: {}) timed out. Last seen: {:?}, Timeout duration: {:?}. Removing...",
                    client_session.addr,
                    client_id,
                    last_seen_dt,
                    self.client_timeout_duration
                );
                // remove_client 是一个 async fn，需要 .await
                // 它也不再返回 Option，所以不需要 if let
                self.connection_manager.remove_client(&client_id).await;
                info!("[HeartbeatMonitor] Finished remove_client call for timed out client {}.", client_id);
            } else {
                debug!("[HeartbeatMonitor] Client {} (ID: {}) is still active. Last seen: {:?}", client_session.addr, client_id, last_seen_dt);
            }
        }
    }
} 