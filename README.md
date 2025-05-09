# SatPlatform - 卫星测试平台

## 1. 项目概览

**当前状态：阶段零 (P0) - 核心通信基础设施搭建已完成。** 各应用组件骨架已建立，`common_models` 库完成了基础数据模型的定义，`rust_websocket_utils` 库完成了 WebSocket 通信基础的封装。各个应用可以独立启动并展示默认页面或基础功能。

SatPlatform 是一个综合性的卫星测试与验证平台，旨在为卫星测试的各个阶段提供支持，包括任务管理、环境准备、单体设备测试、链路联调测试以及数据归档与查询。平台采用现代化的技术栈，构建包括云端服务、中心控制端和现场移动端在内的分布式系统，通过实时通信与数据同步，确保测试流程的高效与协同。

## 2. 核心应用组件

平台主要由以下三个核心应用组件构成：

*   **`sat_cloud_service_desktop` (云端服务应用 / 未来将分离为 `sat_cloud_service_backend`)**:
    *   **角色**: 作为系统的中心枢纽，负责数据管理、实时通信中转、任务分发与状态同步。
    *   **功能**:
        *   提供 RESTful API 用于管理项目信息、测试任务、测试数据等。
        *   内置 WebSocket 服务器，处理来自中心控制端和现场移动端的长连接请求，管理客户端会话 (`ClientSession`)、客户端分组 (基于任务，将中心端和现场端配对) 和消息路由 (`MessageRouter`)。
        *   实现心跳机制 (`HeartbeatMonitor`) 以维持与客户端的连接健康。
        *   在配对的客户端组内同步关键测试数据和指令 (`DataSynchronizer`)。
        *   集成消息队列 (MQ) 服务，用于异步通知（如新任务下发）。
        *   负责测试数据的持久化存储与管理。
    *   **技术**: Rust 后端 (Tauri 桌面应用承载，未来分离为独立 Rust 服务)，`tokio` 异步处理，`serde` 序列化，`actix-web`/`axum` (用于独立服务)，`reqwest`, `log`, `thiserror`/`anyhow`, `tokio-tungstenite` (通过 `rust_websocket_utils`)。

*   **`sat_control_center_desktop` (中心控制端应用)**:
    *   **角色**: 操作员使用的桌面应用程序，用于任务监控、测试控制和数据分析。
    *   **功能**:
        *   通过 WebSocket 和 HTTP API 与云端服务通信。
        *   接收和管理测试任务：获取任务列表、查看任务详情、接收新任务通知。
        *   监控现场环境准备状态。
        *   发起和管理单体设备测试：选择设备、发送测试指令 (如启动、检查状态)、接收现场反馈、确认测试步骤。
        *   发起和管理链路联调测试：定义测试链路、下发联调指令、监控现场执行状态、进行最终确认。
        *   查询和展示历史测试数据与报告。
    *   **技术**: Angular 前端 (`18.2.4`)，TypeScript，Tauri (Rust 本地后端与通信桥梁)，RxJS，`@angular/common/http`，Reactive Forms，`@angular/router`，NG-ZORRO，ECharts。

*   **`sat_on_site_mobile` (现场移动端应用)**:
    *   **角色**: 现场工程师使用的移动应用程序，用于接收指令、执行操作、反馈数据。
    *   **功能**:
        *   通过 WebSocket 和 HTTP API 与云端服务通信。
        *   接收和管理测试任务。
        *   辅助现场环境准备：获取项目资料（设备清单、点位信息、调试步骤），记录前置检查项，发送"环境就绪"通知。
        *   执行单体设备测试：接收中心端指令，（模拟）执行测试操作，反馈测试状态和数据。
        *   执行链路联调测试：接收联调指令，（模拟）执行链路步骤，反馈执行状态。
        *   将测试过程数据上传至云端服务。
    *   **技术**: Angular 前端 (`18.2.4`)，TypeScript，Tauri (Rust 本地后端与通信桥梁，适配 iOS/Android)，其余同中心控制端。

## 3. 关键功能特性

*   **任务管理与分发**:
    *   云端创建和分配测试任务。
    *   通过消息队列和 WebSocket 实时将任务通知推送至相关客户端。
    *   中心端和现场端能够查看任务列表和详细信息。

*   **环境准备协同**:
    *   现场端从云端获取项目配置和设备信息。
    *   现场工程师在移动端确认各项前置检查。
    *   "环境就绪"状态通过 WebSocket 实时同步到中心控制端。

*   **单体设备测试**:
    *   中心端远程选择设备并发起测试指令。
    *   指令通过云端 WebSocket 实时下发至现场移动端。
    *   现场移动端（模拟）执行测试，并将过程状态和结果实时反馈给中心端。
    *   中心端对测试步骤进行确认，形成闭环操作。

*   **链路联调测试**:
    *   中心端定义联调链路（涉及设备、动作序列）。
    *   联调指令通过云端下发，现场端接收并（模拟）执行。
    *   各步骤状态和监控数据在三端间实时同步。
    *   中心端对整个链路测试结果进行最终确认。

*   **实时通信与数据同步**:
    *   基于 WebSocket 实现低延迟的双向通信。
    *   使用自定义的 `WsMessage` 结构 (包含 `message_type` 和 `payload`) 进行标准化消息传输。
    *   云端服务负责在配对的测试小组（中心端与现场端）之间安全、可靠地中转和同步测试指令与数据。

*   **数据记录与查询**:
    *   所有测试指令、反馈、结果和关键操作都将被记录并归档至云端数据库。
    *   中心端提供接口查询历史测试数据，为问题追溯和报告生成提供支持。

*   **模块化与可扩展性**:
    *   通过共享库 `rust_websocket_utils` 和 `common_models` 提升代码复用性。
    *   云端服务设计考虑未来分离为独立的微服务。
    *   Tauri 框架支持跨平台部署。

## 4. 系统架构简述

平台采用典型的三层/多端分布式架构：

1.  **云端服务 (`sat_cloud_service_desktop` / `sat_cloud_service_backend`)**:
    *   作为系统的"大脑"，处理所有业务逻辑、数据持久化和通信协调。
    *   通过 RESTful API 暴露结构化数据接口。
    *   通过 WebSocket API 提供实时交互通道。
    *   通过消息队列（MQ）实现服务内部及对外的异步消息通知。

2.  **中心控制端 (`sat_control_center_desktop`)**:
    *   胖客户端应用程序，与云端服务通过 HTTPS (REST API) 和 WSS (WebSocket) 进行安全通信。
    *   负责测试流程的编排、监控与高级分析。

3.  **现场移动端 (`sat_on_site_mobile`)**:
    *   轻量级移动应用，同样通过 HTTPS 和 WSS 与云端服务通信。
    *   专注于现场操作的便捷性和实时数据反馈。

**通信流程**:
*   客户端（中心端/现场端）通过 WebSocket 连接到云端服务，并根据任务ID加入特定的通信组。
*   指令和数据在组内通过云端服务进行安全转发。
*   批量数据查询和非实时操作通过 RESTful API 进行。

## 5. 技术栈

### 5.1. 核心技术版本
*   **Angular**: `18.2.4`
*   **Tauri API**: `^2.5.0` (for `@tauri-apps/api`)
*   **Tauri CLI**: `^2.0.0`
*   **Rust**: 最新稳定版

### 5.2. 前端 (Angular - `sat_control_center_desktop`, `sat_on_site_mobile`)
*   **语言**: TypeScript
*   **核心库/特性**:
    *   RxJS: 响应式编程，处理异步事件流。
    *   `@angular/common/http` (`HttpClient`): 与云端 RESTful API 交互。
    *   Reactive Forms (`FormGroup`, `FormControl`): 构建动态表单。
    *   `@angular/router`: 实现单页应用导航。
    *   NG-ZORRO (`ng-zorro-antd`): UI 组件库，提供丰富的界面元素。
    *   ECharts: 数据可视化，用于图表展示。
    *   `ChangeDetectionStrategy.OnPush`: 优化变更检测性能。
    *   状态管理: 主要使用 RxJS `BehaviorSubject` / `Subject` 实现服务级和局部状态管理。

### 5.3. 后端 (Rust - 云端服务, Tauri 各端本地逻辑)
*   **核心库/特性**:
    *   `tokio`: 异步运行时。
    *   `serde` (`serde_json`): 数据序列化与反序列化。
    *   `actix-web` / `axum`: (用于未来独立云端服务) 高性能 Web 框架。
    *   `reqwest`: HTTP 客户端，用于服务间通信或调用外部 API。
    *   `log` + `env_logger`/`tracing-subscriber`: 日志记录。
    *   `thiserror` / `anyhow`: 错误处理。
    *   `tokio-tungstenite`: WebSocket 实现基础 (通过 `rust_websocket_utils` 封装)。
    *   `uuid`: 生成唯一标识符。
    *   `chrono`: 日期与时间处理。
    *   (消息队列客户端，如 `lapin` for RabbitMQ 或 Redis 客户端)

### 5.4. 桌面与移动应用框架 (Tauri)
*   使用 `@tauri-apps/api` (v2) 实现前端 Angular 与后端 Rust 逻辑的交互 (`invoke`, `event.listen`, `event.emit`)。
*   利用 Tauri API 实现文件系统操作、对话框、原生通知等。
*   针对 `sat_on_site_mobile` 进行 iOS 和 Android 平台的适配与打包。

### 5.5. 共享组件
*   **`rust_websocket_utils` (Rust 库)**: 封装了通用的 WebSocket 客户端和服务端传输层逻辑，包括 `WsMessage` 结构定义、连接管理、消息收发等。被云端、中心端和现场端的 Rust 部分共用。 **(阶段零已完成基础封装，包括服务端和客户端的 TransportLayer 以及基础的 Send/Receive 功能)**
*   **`common_models` (Rust Crate - 遵循项目规则)**: 定义了跨应用共享的 Rust 数据结构（如各种 DTOs、Payloads、枚举），这些结构派生 `Serialize`, `Deserialize`, `Debug`, `Clone`。其对应的 TypeScript 版本在 Angular 前端使用。 **(阶段零已完成核心数据结构如 `WsMessage` 和示例 `EchoPayload` 的定义)**

## 6. 如何开始

### 6.1. 先决条件
*   安装 Rust (最新稳定版)
*   安装 Node.js 和 npm (或 yarn)
*   安装 Angular CLI: `npm install -g @angular/cli@18.2.4`
*   安装 Tauri CLI: `cargo install tauri-cli --version "^2.0.0"` (或根据项目具体版本)

### 6.2. 克隆仓库
```sh
git clone <repository-url> # 请替换为实际的仓库 URL
cd SatPlatform
```

### 6.3. 构建与运行子项目

各子项目可以独立进行开发和运行。

**a) 共享库 (通常作为其他项目的依赖自动构建)**
```sh
# 进入 common_models 目录进行单独构建 (可选)
cd common_models
cargo build
cd ..

# 进入 rust_websocket_utils 目录进行单独构建 (可选)
cd rust_websocket_utils
cargo build
cd ..
```

**b) SatCloudService (云端服务 - 初期为 Tauri 应用)**
```sh
cd SatCloudService
npm install # 或 yarn install
cargo tauri dev
```
*   **预期：** Tauri 应用窗口成功启动并显示默认的 Angular 欢迎页面。

**c) SatControlCenter (中心控制端 - Tauri 应用)**
```sh
cd SatControlCenter
npm install # 或 yarn install
cargo tauri dev
```
*   **预期：** Tauri 应用窗口成功启动并显示默认的 Angular 欢迎页面。

**d) SatOnSiteMobile (现场移动端 - Tauri 应用)**
```sh
cd SatOnSiteMobile
npm install # 或 yarn install
# 桌面开发模式
cargo tauri dev
# 若要针对移动端，请确保已配置好相应的 tauri.conf.json 和移动开发环境
# cargo tauri android dev (或 run)
# cargo tauri ios dev (或 run)
```
*   **预期：** Tauri 应用窗口 (桌面) 或移动应用 (模拟器/真机) 成功启动并显示默认的 Angular 欢迎页面。

---

**注意:** 上述命令假设您已在各项目的 `src-tauri/Cargo.toml` 中正确配置了对本地 `common_models` 和 `rust_websocket_utils` 库的 `path` 依赖。例如：
```toml
[dependencies]
common_models = { path = "../../common_models" }
rust_websocket_utils = { path = "../../rust_websocket_utils" }
# ...其他依赖
```

## 7. 项目目录
common_models/
├── src/
│   ├── lib.rs                      # 声明模块 (pub mod task_info 等)
│   ├── task_info.rs                # TaskInfo, TaskStatus, TaskAssignment (任务信息、状态、分配等)
│   ├── project_details.rs          # ProjectDetails, DeviceInfo, PointInfo, DebugStepInfo (项目详情、设备、点位、调试步骤等)
│   ├── test_data.rs                # SingleTestFeedback, LinkTestResult (测试反馈、结果等)
│   ├── ws_payloads.rs              # WebSocket 消息体中的具体业务载荷结构 (如 EnvironmentReadyNotification, SingleTestCommand, RegisterPayload 等)
│   ├── api_models.rs               # API 请求/响应体中可能共用的数据结构
│   └── error.rs                    # (可选) 通用错误类型/代码
└── Cargo.toml                      # 依赖: serde, uuid, chrono 等

sat_cloud_service_desktop/
├── src/                            # Angular 前端 (管理界面)
│   ├── app/                        # 管理界面的组件、服务、模块
│   ├── assets/
│   ├── environments/
│   ├── index.html
│   ├── main.ts
│   └── styles.css
├── src-tauri/                      # Rust 后端 (Tauri)
│   ├── src/
│   │   ├── main.rs                 # Tauri 入口点, 服务初始化与启动
│   │   ├── config.rs               # 配置加载 (端口, DB 连接字符串, MQ 地址等)
│   │   ├── state.rs                # 共享应用状态 (数据库连接池, MQ 连接, ConnectionManager 等)
│   │   ├── error.rs                # 服务端错误类型定义
│   │   ├── api/                    # RESTful API 模块
│   │   │   ├── mod.rs
│   │   │   ├── routes.rs           # API 路由定义 (使用 Actix/Axum 等)
│   │   │   ├── task_handler.rs     # 处理任务相关 API
│   │   │   ├── project_handler.rs  # 处理项目相关 API
│   │   │   └── test_data_handler.rs # 处理测试数据上传/查询 API
│   │   ├── ws_server/              # WebSocket 服务端模块
│   │   │   ├── mod.rs
│   │   │   ├── service.rs          # WS 服务设置与运行
│   │   │   ├── connection_manager.rs # 管理 ClientSession, Group 逻辑
│   │   │   ├── message_router.rs   # 处理和分发入站 WebSocket 消息
│   │   │   ├── heartbeat_monitor.rs # 处理心跳检测和超时
│   │   │   └── data_synchronizer.rs # 处理组内数据同步逻辑
│   │   ├── db/                     # 数据库交互模块
│   │   │   ├── mod.rs
│   │   │   ├── connection.rs       # 数据库连接池设置
│   │   │   ├── task_repo.rs        # 任务数据的仓储操作
│   │   │   └── ...                 # 其他数据的仓储操作
│   │   ├── mq/                     # 消息队列交互模块
│   │   │   ├── mod.rs
│   │   │   ├── connection.rs       # MQ 连接管理
│   │   │   ├── publisher.rs        # 消息发布逻辑
│   │   │   └── subscriber.rs       # 消息订阅和处理逻辑 (例如: 收到新任务通知后转发给 WS)
│   │   └── commands.rs             # (可选) 如果管理 UI 需要通过 Tauri 命令与后端交互
│   ├── build.rs                    # (可选) 构建脚本
│   └── tauri.conf.json             # Tauri 配置
├── node_modules/                   # Node.js 依赖
├── .gitignore
├── Cargo.toml                      # Rust 依赖 (tauri, common_models, rust_websocket_utils, web 框架, DB 驱动, MQ 客户端等)
└── package.json                    # 前端依赖


sat_control_center_desktop/
├── src/                            # Angular 前端
│   ├── app/
│   │   ├── core/                   # 核心服务、守卫、拦截器
│   │   │   ├── services/
│   │   │   │   ├── auth.service.ts
│   │   │   │   ├── task-state.service.ts  # 任务状态管理 (使用 RxJS)
│   │   │   │   ├── websocket.service.ts # 封装 Tauri WS 事件监听和命令调用
│   │   │   │   └── api.service.ts       # 封装 Tauri API 命令调用
│   │   │   └── guards/
│   │   ├── features/               # 按功能划分的模块/组件
│   │   │   ├── task-list/
│   │   │   ├── task-detail/
│   │   │   ├── single-testing/      # 单体测试界面
│   │   │   ├── link-testing/        # 链路测试界面
│   │   │   └── history-viewer/      # 测试历史查看界面
│   │   ├── shared/                 # 共享 UI 组件、指令、管道
│   │   ├── app.component.ts
│   │   ├── app.module.ts
│   │   └── app-routing.module.ts
│   ├── assets/
│   ├── environments/
│   ├── index.html
│   ├── main.ts
│   └── styles.css
├── src-tauri/                      # Rust 后端 (Tauri)
│   ├── src/
│   │   ├── main.rs                 # Tauri 入口点, 命令注册
│   │   ├── commands/               # Tauri 命令模块 (按功能组织) ✨
│   │   │   ├── mod.rs
│   │   │   ├── general_cmds.rs     # 通用命令 (如连接, 发送 WS 消息)
│   │   │   ├── task_cmds.rs        # 任务相关命令 (获取列表/详情)
│   │   │   ├── test_cmds.rs        # 测试相关命令 (发起测试/确认步骤)
│   │   │   └── data_cmds.rs        # 数据相关命令 (获取项目详情/上传数据)
│   │   ├── ws_client/              # WebSocket 客户端模块 ✨
│   │   │   ├── mod.rs
│   │   │   └── service.rs          # 管理 WS 连接, 收发消息, 发射 Tauri 事件
│   │   ├── api_client/             # API 客户端模块 ✨
│   │   │   ├── mod.rs
│   │   │   └── service.rs          # 封装调用云端 API 的函数 (使用 reqwest)
│   │   ├── plc_comms/              # PLC 通信模块 (根据架构图占位)
│   │   │   └── mod.rs
│   │   ├── state.rs                # Rust 后端状态管理 (WS 连接状态, API Client 实例等)
│   │   ├── config.rs               # 配置加载 (云端 URL 等)
│   │   ├── event.rs                # Tauri 事件定义与处理 (与 Angular 交互) ✨
│   │   └── error.rs                # 客户端侧错误类型定义
│   ├── build.rs
│   └── tauri.conf.json             # Tauri 配置
├── node_modules/
├── .gitignore
├── Cargo.toml                      # Rust 依赖 (tauri, common_models, rust_websocket_utils, reqwest 等)
└── package.json


sat_on_site_mobile/
├── src/                            # Angular 前端 (移动端优化)
│   ├── app/
│   │   ├── core/                   # 核心服务 (类似中心侧)
│   │   │   ├── services/
│   │   │   └── ...
│   │   ├── features/               # 功能模块 (移动端特定视图)
│   │   │   ├── task-list/
│   │   │   ├── task-detail/
│   │   │   ├── site-preparation/    # 现场准备界面 ✨
│   │   │   ├── onsite-single-test/  # 现场单体测试执行界面 ✨
│   │   │   ├── onsite-link-test/    # 现场链路测试执行界面 ✨
│   │   │   └── ...
│   │   ├── shared/                 # 共享 UI 组件等
│   │   ├── app.component.ts
│   │   ├── app.module.ts
│   │   └── app-routing.module.ts
│   ├── assets/
│   ├── environments/
│   ├── index.html
│   ├── main.ts
│   └── styles.scss                 # 使用 SCSS 可能更有利于移动端样式管理
├── src-tauri/                      # Rust 后端 (Tauri)
│   ├── src/
│   │   ├── main.rs                 # Tauri 入口点
│   │   ├── commands/               # Tauri 命令模块 (部分命令实现可能不同) ✨
│   │   │   ├── mod.rs
│   │   │   ├── general_cmds.rs
│   │   │   ├── task_cmds.rs
│   │   │   ├── test_cmds.rs        # 例如: confirm_environment_ready_on_site, execute_simulated_single_test
│   │   │   └── data_cmds.rs
│   │   ├── ws_client/              # WebSocket 客户端模块 (类似中心侧) ✨
│   │   │   ├── mod.rs
│   │   │   └── service.rs
│   │   ├── api_client/             # API 客户端模块 (类似中心侧) ✨
│   │   │   ├── mod.rs
│   │   │   └── service.rs
│   │   ├── device_comms/           # 设备通信模块 ✨
│   │   │   └── mod.rs
│   │   ├── mobile_specific/        # 用于调用 Tauri 移动端原生 API 的模块 (阶段 13) ✨
│   │   │   └── mod.rs
│   │   ├── state.rs                # Rust 后端状态管理
│   │   ├── config.rs               # 配置加载
│   │   ├── event.rs                # Tauri 事件处理 ✨
│   │   └── error.rs                # 客户端侧错误类型
│   ├── build.rs
│   └── tauri.conf.json             # Tauri 配置 (包含 mobile 配置节) ✨
├── node_modules/
├── .gitignore
├── Cargo.toml                      # Rust 依赖 (类似中心侧, 可能增加 tauri 移动端 API 插件依赖)
└── package.json

*(如果项目接受贡献，可以在此添加相关说明)*