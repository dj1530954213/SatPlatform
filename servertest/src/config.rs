use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::env;
use std::path::Path;

/// WebSocket 服务的默认主机地址
pub const DEFAULT_WS_HOST: &str = "0.0.0.0";
/// WebSocket 服务的默认端口号
pub const DEFAULT_WS_PORT: u16 = 8088;

/// WebSocket 服务端详细配置结构体
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket 服务绑定的主机地址
    pub host: String,
    /// WebSocket 服务监听的端口号
    pub port: u16,
    /// 心跳检查的间隔时间（单位：秒）
    pub heartbeat_check_interval_seconds: u64,
    /// 客户端超时时间（单位：秒）
    pub client_timeout_seconds: u64,
}

// 为 WebSocketConfig 实现 Default trait
impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),      // 默认监听所有网络接口
            port: 8088,                         // 默认监听 8088 端口
            heartbeat_check_interval_seconds: 15, // 默认每 15 秒检查一次心跳
            client_timeout_seconds: 60,         // 默认客户端 60 秒无响应则超时
        }
    }
}

/// 应用的主配置结构体
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    /// WebSocket 服务的相关配置
    pub websocket: WebSocketConfig,
    // 在此可以添加其他配置项，例如：
    // pub database: DatabaseConfig,
    // pub message_queue: MessageQueueConfig,
}

// 全局静态应用配置实例
static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// 加载或创建应用配置文件
fn load_or_create_config() -> AppConfig {
    // 获取配置文件路径，优先使用当前目录
    let config_file_path = get_config_file_path();

    // 尝试读取配置文件
    match fs::read_to_string(&config_file_path) {
        Ok(content) => { // 文件读取成功
            match serde_json::from_str::<AppConfig>(&content) {
                Ok(config) => { // 反序列化成功
                    info!("[配置模块] 已成功从配置文件 {:?} 加载应用配置。", config_file_path);
                    config // 返回从文件加载到的配置
                }
                Err(e) => { // 反序列化失败
                    warn!(
                        "[配置模块] 警告：从 {:?} 反序列化配置失败: {}. 文件可能已损坏。将使用默认配置并尝试覆盖。",
                        config_file_path, e
                    );
                    let default_config = AppConfig::default();
                    save_config(&default_config, &config_file_path);
                    default_config
                }
            }
        }
        Err(e) => { // 文件读取失败
            info!(
                "[配置模块] 未在 {:?} 找到配置文件或读取时发生错误 (错误: {}). 将使用默认配置并尝试创建新文件。",
                config_file_path, e
            );
            let default_config = AppConfig::default();
            save_config(&default_config, &config_file_path);
            default_config
        }
    }
}

/// 获取配置文件路径
fn get_config_file_path() -> PathBuf {
    // 首先尝试当前目录
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config_file_path = current_dir.join("app_settings.json");
    
    // 检查当前目录是否可写
    if Path::new(&config_file_path).exists() || fs::metadata(&current_dir).map(|m| m.permissions().readonly()).unwrap_or(true) == false {
        return config_file_path;
    }
    
    // 如果当前目录不可写，则尝试使用用户主目录
    if let Ok(home) = env::var("HOME") {
        let home_config = PathBuf::from(home).join(".config").join("servertest");
        if !home_config.exists() {
            let _ = fs::create_dir_all(&home_config);
        }
        return home_config.join("app_settings.json");
    } else if let Ok(userprofile) = env::var("USERPROFILE") {
        // Windows环境
        let home_config = PathBuf::from(userprofile).join("AppData").join("Local").join("servertest");
        if !home_config.exists() {
            let _ = fs::create_dir_all(&home_config);
        }
        return home_config.join("app_settings.json");
    }
    
    // 最后返回当前目录的配置文件路径，即使可能写入失败
    config_file_path
}

/// 保存配置到文件
fn save_config(config: &AppConfig, path: &PathBuf) {
    // 确保目录存在
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!("[配置模块] 错误：创建配置目录 {:?} 失败: {}", parent, e);
                return;
            }
        }
    }
    
    // 尝试将 AppConfig 实例美化并序列化为 JSON 字符串
    match serde_json::to_string_pretty(config) {
        Ok(content) => { // 序列化成功
            // 尝试将序列化后的 JSON 字符串内容写入到指定的文件路径
            if let Err(e) = fs::write(path, content) {
                warn!("[配置模块] 错误：将配置写入文件 {:?} 时失败: {}", path, e);
            } else {
                info!("[配置模块] 已成功将当前配置（可能是默认配置）保存到 {:?}.", path);
            }
        }
        Err(e) => { // 序列化为 JSON 字符串失败
            warn!("[配置模块] 错误：序列化配置信息以便保存时失败: {}", e);
        }
    }
}

/// 初始化全局应用配置
pub fn init_config() {
    let loaded_config = load_or_create_config();
    if APP_CONFIG.set(loaded_config).is_err() {
        // 使用 eprintln! 因为这表示一个潜在的严重初始化问题
        eprintln!("[配置模块] 严重警告：全局应用配置 APP_CONFIG 已经被初始化过了。");
        // 也使用 warn! 记录到标准日志流
        warn!("[配置模块] 全局应用配置 APP_CONFIG 已被初始化，本次 init_config 调用未覆盖已有配置。请检查初始化流程。");
    }
    info!("[配置模块] 应用配置已成功初始化完毕。");
}

/// 获取已加载的全局应用配置
pub fn get_config() -> &'static AppConfig {
    APP_CONFIG.get().expect("[配置模块] 全局应用配置尚未初始化，请先调用 init_config()")
} 