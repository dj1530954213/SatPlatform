// SatCloudService/src-tauri/src/config.rs

use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::Manager;

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

// 全局应用配置实例
static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// WebSocket 服务端配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebSocketConfig {
    pub host: String,
    pub port: u16,
    pub heartbeat_check_interval_seconds: u64,
    pub client_timeout_seconds: u64,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8088,
            heartbeat_check_interval_seconds: 15,
            client_timeout_seconds: 30,
        }
    }
}

/// 应用的主配置结构体
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    pub websocket: WebSocketConfig,
}

/// 加载或创建应用配置。
fn load_or_create_config(app_handle: &tauri::AppHandle) -> AppConfig {
    // app_config_dir() 返回 Result<PathBuf, tauri::Error>
    let config_dir = match app_handle.path().app_config_dir() { 
        Ok(path) => path, // 处理 Ok(path)
        Err(e) => {      // 处理 Err(e)
            eprintln!("[Config] Error getting app config directory: {}. Using default config.", e);
            return AppConfig::default();
        }
    };

    let app_specific_config_dir = config_dir.join("SatCloudService");
    if !app_specific_config_dir.exists() {
        if let Err(e) = fs::create_dir_all(&app_specific_config_dir) {
            eprintln!(
                "[Config] Error creating app config directory {:?}: {}",
                app_specific_config_dir,
                e
            );
            return AppConfig::default();
        }
    }

    let config_file_path = app_specific_config_dir.join("app_settings.json");

    match fs::read_to_string(&config_file_path) {
        Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
            Ok(config) => {
                info!("[Config] Successfully loaded configuration from {:?}", config_file_path);
                config
            }
            Err(e) => {
                warn!(
                    "[Config] Error deserializing config from {:?}: {}. Using default config and attempting to save.",
                    config_file_path, e
                );
                let default_config = AppConfig::default();
                save_config(&default_config, &config_file_path);
                default_config
            }
        },
        Err(_e) => { 
            info!(
                "[Config] Config file not found or unreadable at {:?}. Using default config and attempting to create it.",
                config_file_path
            );
            let default_config = AppConfig::default();
            save_config(&default_config, &config_file_path);
            default_config
        }
    }
}

/// 将配置保存到指定路径。
fn save_config(config: &AppConfig, path: &PathBuf) {
    match serde_json::to_string_pretty(config) {
        Ok(content) => {
            if let Err(e) = fs::write(path, content) {
                warn!("[Config] Error writing default config to {:?}: {}", path, e);
            }
        }
        Err(e) => {
            warn!("[Config] Error serializing default config for saving: {}", e);
        }
    }
}

/// 初始化应用配置（应在应用启动时调用一次）。
pub fn init_config(app_handle: &tauri::AppHandle) { 
    let loaded_config = load_or_create_config(app_handle);
    if APP_CONFIG.set(loaded_config).is_err() {
        eprintln!("[Config] Error: APP_CONFIG was already initialized.");
    }
    info!("[Config] Application configuration initialized.");
}

/// 获取对已加载应用配置的引用。
pub fn get_config() -> &'static AppConfig {
    APP_CONFIG.get().expect("[Config] APP_CONFIG not initialized. Call init_config first.")
} 