// SatControlCenter/src-tauri/src/config.rs

//! 中心端配置信息。

// 暂时为空，后续会添加配置加载和管理逻辑。

use serde::Deserialize;

// WebSocket 客户端配置
#[derive(Debug, Deserialize, Clone)]
pub struct WsClientConfig {
    pub cloud_ws_url: String, // 云端 WebSocket 服务 URL
}

impl WsClientConfig {
    // 默认配置或从 Tauri 配置加载的示例
    // 在实际应用中，这里应该从 tauri.conf.json 或环境变量加载
    pub fn load() -> Self {
        log::info!("加载 WebSocket 客户端配置...");
        // 示例 URL，实际项目中应可配置
        // TODO: 从 Tauri 配置文件或环境变量加载 URL
        let default_url = "ws://127.0.0.1:8088".to_string(); 
        log::info!("默认云端 WebSocket URL: {}", default_url);
        Self {
            cloud_ws_url: default_url,
        }
    }
}

// 可以在这里添加更多应用级别的配置 