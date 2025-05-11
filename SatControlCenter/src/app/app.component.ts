import { Component, OnInit, OnDestroy, ChangeDetectorRef } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// WebSocket 连接状态事件的 Payload (与 Rust 端 WsConnectionStatusEvent 对应)
interface WsConnectionStatusEventPayload {
  connected: boolean;
  client_id?: string | null;
  error_message?: string | null;
}

// 客户端注册状态事件的 Payload
interface WsRegistrationStatusEventPayload {
  success: boolean;
  message?: string | null;
  group_id?: string | null; // 确认注册到的组ID
  task_id?: string | null;  // 确认注册到的任务ID
}

// 伙伴客户端状态事件的 Payload
interface WsPartnerStatusEventPayload {
  partner_role: string; // 例如 'ControlCenter' 或 'OnSiteMobile'
  is_online: boolean;
}

// 本地任务状态更新事件的 Payload (TaskDebugState 的具体结构后续定义)
interface LocalTaskStateUpdatedEventPayload {
  new_state: any; // 暂时用 any，后续应替换为 TaskDebugState 接口
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit, OnDestroy {
  private unlistenStatus?: UnlistenFn;
  private unlistenRegistration?: UnlistenFn;
  private unlistenPartnerStatus?: UnlistenFn;
  private unlistenTaskStateUpdate?: UnlistenFn;
  private unlistenEcho?: UnlistenFn;

  // 用于UI绑定的属性
  groupId: string = 'testgroup1'; // 默认值，方便测试
  taskId: string = 'task101';    // 默认值，方便测试

  connectionStatusMessage: string = '未连接';
  registrationStatusMessage: string = '未注册';
  partnerStatusMessage: string = '伙伴状态未知';
  taskStateString: string = '任务状态未初始化';

  // --- Echo 功能相关属性 ---
  echoInput: string = '你好，云端！来自 SatControlCenter。'; // 默认 Echo 内容
  echoResponseMessage: string = '等待 Echo 回复...';

  constructor(private cdr: ChangeDetectorRef) {
    console.log('AppComponent 构造函数: SatControlCenter');
  }

  async ngOnInit(): Promise<void> {
    console.log('AppComponent ngOnInit: SatControlCenter - 正在设置 Tauri 监听器并调用命令...');

    // 1. 监听 WebSocket 连接状态事件
    if (!this.unlistenStatus) {
      try {
        this.unlistenStatus = await listen<WsConnectionStatusEventPayload>('ws_connection_status', (event) => {
          console.log('接收到 Tauri 事件 "ws_connection_status":', event.payload);
          if (event.payload.connected) {
            this.connectionStatusMessage = `已连接到云端。客户端 ID: ${event.payload.client_id || '未知'}. ${event.payload.error_message || ''}`.trim();
            // 连接成功后，可以考虑自动触发注册（如果需要）
            // 或提示用户进行注册
          } else {
            this.connectionStatusMessage = `连接已断开或失败: ${event.payload.error_message || '未知原因'}`;
          }
        });
        console.log('成功监听 "ws_connection_status" 事件。');
      } catch (error) {
        console.error('设置 "ws_connection_status" 监听器失败:', error);
        this.connectionStatusMessage = '设置连接状态监听器失败';
      }
    }

    // 2. 监听客户端注册状态事件
    if (!this.unlistenRegistration) {
      try {
        this.unlistenRegistration = await listen<WsRegistrationStatusEventPayload>('ws_registration_status_event', (event) => {
          console.log('接收到 Tauri 事件 "ws_registration_status_event":', event.payload);
          if (event.payload.success) {
            this.registrationStatusMessage = `注册成功! 组: ${event.payload.group_id}, 任务: ${event.payload.task_id}. ${event.payload.message || ''}`.trim();
          } else {
            this.registrationStatusMessage = `注册失败: ${event.payload.message || '未知原因'}`;
          }
        });
        console.log('成功监听 "ws_registration_status_event" 事件。');
      } catch (error) {
        console.error('设置 "ws_registration_status_event" 监听器失败:', error);
        this.registrationStatusMessage = '设置注册状态监听器失败';
      }
    }

    // 3. 监听伙伴客户端状态事件
    if (!this.unlistenPartnerStatus) {
      try {
        this.unlistenPartnerStatus = await listen<WsPartnerStatusEventPayload>('ws_partner_status_event', (event) => {
          console.log('接收到 Tauri 事件 "ws_partner_status_event":', event.payload);
          this.partnerStatusMessage = `伙伴 (${event.payload.partner_role}) 状态: ${event.payload.is_online ? '在线' : '离线'}`;
        });
        console.log('成功监听 "ws_partner_status_event" 事件。');
      } catch (error) {
        console.error('设置 "ws_partner_status_event" 监听器失败:', error);
        this.partnerStatusMessage = '设置伙伴状态监听器失败';
      }
    }

    // 4. 监听本地任务状态更新事件
    if (!this.unlistenTaskStateUpdate) {
      try {
        this.unlistenTaskStateUpdate = await listen<LocalTaskStateUpdatedEventPayload>('local_task_state_updated_event', (event) => {
          console.log('接收到 Tauri 事件 "local_task_state_updated_event":', event.payload);
          this.taskStateString = JSON.stringify(event.payload.new_state, null, 2);
          this.cdr.detectChanges();
        });
        console.log('成功监听 "local_task_state_updated_event" 事件。');
      } catch (error) {
        console.error('设置 "local_task_state_updated_event" 监听器失败:', error);
        this.taskStateString = '设置任务状态监听器失败';
        this.cdr.detectChanges();
      }
    }

    // 5. 监听 Echo 回复事件 (P2.2.1)
    if (!this.unlistenEcho) {
      try {
        this.unlistenEcho = await listen<any>('echo_response_event', (event: any) => { // 使用 any 暂时匹配后端事件结构
          console.log('接收到Tauri事件 "echo_response_event":', event.payload);
          // 假设 event.payload 直接就是 { content: string } 或类似 EchoResponseEventPayload 的结构
          this.echoResponseMessage = event.payload?.content ? `收到 Echo 回复: ${event.payload.content}` : JSON.stringify(event.payload);
          this.cdr.detectChanges();
        });
        console.log('成功监听 "echo_response_event" 事件。');
      } catch (e) {
        console.error('设置 "echo_response_event" 监听器失败:', e);
        this.echoResponseMessage = '设置Echo监听器失败';
        this.cdr.detectChanges(); 
      }
    }

    // 初始连接 (通常在用户操作后，或自动)
    // this.connectToCloud(); 
    console.log('ngOnInit 完成。等待用户操作或自动连接触发。');
  }

  // 新增方法，用于连接到云端
  async connectToCloud(isReconnect: boolean = false): Promise<void> {
    const actionType = isReconnect ? '重新调用' : '正在调用';
    try {
      console.log(`${actionType} "connect_to_cloud" 命令，目标 URL: ws://127.0.0.1:8088 ...`);
      const connectionResult: any = await invoke('connect_to_cloud', { url: 'ws://127.0.0.1:8088' });
      console.log(`"connect_to_cloud" 命令${actionType}结果 (可能不代表最终状态，请检查事件):`, connectionResult);
    } catch (error) {
      console.error(`${actionType} "connect_to_cloud" 命令时出错:`, error);
    }
  }

  // 新增方法，供按钮调用以触发注册
  async registerClient(): Promise<void> {
    if (!this.groupId || !this.taskId) {
      this.registrationStatusMessage = '请输入组ID和任务ID后重试。';
      console.error(this.registrationStatusMessage);
      return;
    }
    console.log(`按钮触发：请求注册客户端... 组ID: ${this.groupId}, 任务ID: ${this.taskId}`);
    this.registrationStatusMessage = `尝试注册到组: ${this.groupId}, 任务: ${this.taskId}...`;
    try {
      await invoke('register_client_with_task', { groupId: this.groupId, taskId: this.taskId });
      console.log('"register_client_with_task" 命令已调用。等待事件反馈...');
    } catch (error) {
      console.error('调用 "register_client_with_task" 命令时出错:', error);
      this.registrationStatusMessage = `调用注册命令失败: ${error}`;
    }
  }

  // 新增方法，供按钮调用以触发重新连接
  async triggerReconnect(): Promise<void> {
    console.log('按钮触发：请求重新连接到云端...');
    await this.connectToCloud(true);
  }

  // Echo 功能方法 (P2.2.1)
  async sendEcho(): Promise<void> {
    if (!this.echoInput) {
      this.echoResponseMessage = '请输入要发送的Echo内容。';
      this.cdr.detectChanges();
      return;
    }
    this.echoResponseMessage = '正在发送Echo...';
    this.cdr.detectChanges();
    try {
      console.log(`调用 "send_ws_echo" 命令，内容: ${this.echoInput}`);
      // 假设 SatControlCenter 的 Rust 命令也叫 send_ws_echo
      await invoke('send_ws_echo', { content: this.echoInput }); 
      console.log('"send_ws_echo" 命令已调用。等待事件反馈...');
    } catch (error) {
      console.error('调用 "send_ws_echo" 命令时出错:', error);
      this.echoResponseMessage = `发送Echo失败: ${error}`;
      this.cdr.detectChanges();
    }
  }

  ngOnDestroy(): void {
    console.log('AppComponent ngOnDestroy: SatControlCenter - 正在清理监听器...');
    if (this.unlistenStatus) {
      this.unlistenStatus();
      console.log('已取消监听 "ws_connection_status" 事件。');
    }
    if (this.unlistenRegistration) {
      this.unlistenRegistration();
      console.log('已取消监听 "ws_registration_status_event" 事件。');
    }
    if (this.unlistenPartnerStatus) {
      this.unlistenPartnerStatus();
      console.log('已取消监听 "ws_partner_status_event" 事件。');
    }
    if (this.unlistenTaskStateUpdate) {
      this.unlistenTaskStateUpdate();
      console.log('已取消监听 "local_task_state_updated_event" 事件。');
    }
    // 清理 Echo 监听器 (P2.2.1)
    if (this.unlistenEcho) {
      this.unlistenEcho();
      console.log('已取消监听 "echo_response_event" 事件。');
    }
  }
}
