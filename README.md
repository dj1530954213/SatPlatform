# SatPlatform - 卫星测试平台

## 1. 项目概览

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
*   **`rust_websocket_utils` (Rust 库)**: 封装了通用的 WebSocket 客户端和服务端传输层逻辑，包括 `WsMessage` 结构定义、连接管理、消息收发等。被云端、中心端和现场端的 Rust 部分共用。
*   **`common_models` (Rust Crate - 遵循项目规则)**: 定义了跨应用共享的 Rust 数据结构（如各种 DTOs、Payloads、枚举），这些结构派生 `Serialize`, `Deserialize`, `Debug`, `Clone`。其对应的 TypeScript 版本在 Angular 前端使用。

## 6. 如何开始 (Placeholder)

*(未来添加项目的具体构建、运行和开发指南)*

```sh
# 克隆仓库
git clone <repository-url>
cd SatPlatform

# 启动云端服务 (示例，具体命令待定)
# cd sat_cloud_service_desktop && cargo tauri dev (初期)
# cd sat_cloud_service_backend && cargo run (独立服务后)

# 启动中心控制端 (示例)
# cd sat_control_center_desktop && cargo tauri dev

# 启动现场移动端 (示例)
# cd sat_on_site_mobile && cargo tauri dev (或 cargo tauri android run / cargo tauri ios run)
```

## 7. 贡献指南 (Placeholder)

*(如果项目接受贡献，可以在此添加相关说明)*