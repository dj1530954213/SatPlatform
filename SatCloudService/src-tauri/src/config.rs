// SatCloudService/src-tauri/src/config.rs

//! 服务端配置信息。

use serde::Deserialize;

pub const DEFAULT_WS_HOST: &str = "127.0.0.1";
pub const DEFAULT_WS_PORT: u16 = 8088; // 端口从常用的 8080 修改为 8088 以避免冲突

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)] // 暂时允许未使用的代码，因为在简化的加载函数中未使用此结构体
struct WsPluginSettings { // 用于从 tauri.conf.json 解析插件配置的辅助结构体（当前未启用）
    host: Option<String>,
    port: Option<u16>,
}

/// WebSocket 服务配置结构体
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// WebSocket 服务监听的主机地址
    pub host: String,
    /// WebSocket 服务监听的端口号
    pub port: u16,
}

impl Default for WsConfig {
    fn default() -> Self {
        WsConfig {
            host: DEFAULT_WS_HOST.to_string(),
            port: DEFAULT_WS_PORT,
        }
    }
}

impl WsConfig {
    /// 加载 WebSocket 配置。
    ///
    /// P1.1.1 阶段简化了此函数，以避免解析 `tauri::Config` 时可能出现的问题。
    /// 目前它仅记录日志并使用默认值。
    /// 后续可以扩展为从环境变量或 Tauri 配置文件中读取。
    pub fn load(_app_handle: &tauri::AppHandle) -> Self { // _app_handle 暂时未使用，但保留以备后续扩展
        let default_config = WsConfig::default();
        log::info!(
            "使用默认 WebSocket 配置: host={}, port={}. 从 tauri.conf.json 加载自定义配置的功能已简化。", 
            default_config.host, 
            default_config.port
        );
        default_config
    }
}

// 模块末尾的注释 "暂时为空，后续会添加配置加载和管理逻辑。" 可以移除，因为已有 WsConfig 实现。 