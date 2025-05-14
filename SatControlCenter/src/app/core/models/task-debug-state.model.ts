// common_models/src/task_state.rs (P3.3.1 & P6.1.1 implied structure)
// TaskDebugState 结构体定义: (详细字段参照P6.1.1的TaskDebugState增强描述，确保覆盖预检查、单体测试、联锁测试的详细状态字段，包括各自的ExecutionState Map和当前活动ID等)。
// 定义PreCheckItemExecutionState, SingleTestStepExecutionState, InterlockTestCaseExecutionState等内部状态结构，包含各自的current_status枚举。
// 所有结构体派生Serialize, Deserialize, Debug, Clone。

/**
 * @description 通用的执行状态枚举。
 *              对应 Rust 枚举: `common_models::enums::ExecutionStatus` (或类似名称)
 */
export enum ExecutionStatus {
  Pending = 'PENDING', // 尚未开始
  InProgress = 'IN_PROGRESS', // 正在进行中
  Paused = 'PAUSED', // 已暂停
  CompletedSuccess = 'COMPLETED_SUCCESS', // 成功完成
  CompletedFailure = 'COMPLETED_FAILURE', // 失败完成
  Skipped = 'SKIPPED', // 已跳过
  Aborted = 'ABORTED', // 已中止
  Error = 'ERROR', // 执行中发生错误
  Blocked = 'BLOCKED', // 被前置条件或依赖阻塞
  AwaitingInput = 'AWAITING_INPUT', // 等待用户或外部输入
  NotApplicable = 'NOT_APPLICABLE', // 不适用
}

/**
 * @description 预检查项的执行状态。
 *              对应 Rust 结构体: `PreCheckItemExecutionState`
 */
export interface PreCheckItemExecutionState {
  item_id: string; // 预检查项的唯一ID，关联到模板中的定义
  current_status: ExecutionStatus; // 当前执行状态
  actual_value?: any; // 实际检查值 (根据模板中 input_type 确定具体类型)
  checked_by?: string; // 检查人 (client_id 或 display_name)
  checked_at?: number; // 检查时间戳 (Unix Milliseconds)
  comments?: string; // 备注
  photos?: string[]; // 照片附件的路径或ID (如果适用)
  // P6.1.1 中可能进一步增强的字段
}

/**
 * @description 单体设备测试步骤的执行状态。
 *              对应 Rust 结构体: `SingleTestStepExecutionState`
 */
export interface SingleTestStepExecutionState {
  step_id: string; // 测试步骤的唯一ID，关联到模板中的定义
  current_status: ExecutionStatus;
  command_sent_at?: number; // 命令发送时间
  feedback_received_at?: number; // 反馈接收时间
  feedback_data?: any; // 从现场端接收到的具体反馈数据
  execution_logs?: string[]; // 执行日志
  error_details?: string; // 错误详情
  // P6.1.1 中可能进一步增强的字段
}

/**
 * @description 联锁测试用例的执行状态。
 *              对应 Rust 结构体: `InterlockTestCaseExecutionState`
 */
export interface InterlockTestCaseExecutionState {
  case_id: string; // 测试用例的唯一ID，关联到模板中的定义
  current_status: ExecutionStatus;
  preconditions_met?: boolean; // 前提条件是否满足
  trigger_action_executed_at?: number; // 触发动作执行时间
  outcome_observed_at?: number; // 预期结果观察时间
  observed_outcome_points?: { [pointName: string]: any }; // 观察到的实际点位值
  execution_logs?: string[];
  error_details?: string;
  // P6.1.1 中可能进一步增强的字段
}

/**
 * @description 任务调试的整体状态。
 *              对应 Rust 结构体: `common_models::task_state::TaskDebugState`
 */
export interface TaskDebugState {
  // --- 元数据 ---
  task_id: string; // 核心任务的唯一标识符
  group_id: string; // 当前调试会话组的唯一标识符

  last_updated_by_client_id?: string | null; // 最后更新此状态的客户端ID
  last_updated_by_role?: string | null; // 最后更新此状态的客户端角色 (ClientRole 枚举的字符串形式)
  last_update_timestamp: number; // Unix毫秒时间戳，表示此状态最后更新的时间

  // --- 顶层状态与控制 ---
  overall_task_status: ExecutionStatus; // 整个调试任务的总体状态
  current_activity_type?: 'PreCheck' | 'SingleDeviceTest' | 'InterlockTest' | null; // 当前正在进行的活动类型
  active_pre_check_template_id?: string | null; // (P6) 当前活动的预检查模板ID
  active_single_device_test_template_id?: string | null; // (P6) 当前活动的单体测试模板ID (可能与特定设备关联)
  active_single_device_instance_id?: string | null; // (P6) 当前正在测试的单体设备实例ID (如 "Pump-A01")
  active_interlock_test_template_id?: string | null; // (P6) 当前活动的联锁测试模板ID

  general_debug_notes?: string | null; // 通用调试备注
  custom_shared_data?: { [key: string]: any } | null; // 用于灵活共享自定义数据的JSON对象

  // --- 预检查阶段状态 (P6.1.1 详细定义) ---
  // Key: PreCheckItemDefinition.item_id
  pre_check_items?: { [itemId: string]: PreCheckItemExecutionState } | null;
  current_pre_check_item_id?: string | null; // 当前正在处理或聚焦的预检查项ID
  pre_check_overall_status: ExecutionStatus;

  // --- 单体设备测试阶段状态 (P6.1.1 详细定义) ---
  // Key: SingleDeviceTestStepDefinition.step_id (针对当前活动的设备和模板)
  single_test_steps?: { [stepId: string]: SingleTestStepExecutionState } | null;
  current_single_test_step_id?: string | null; // 当前单体测试步骤ID
  single_test_overall_status: ExecutionStatus;

  // --- 联锁测试阶段状态 (P6.1.1 详细定义) ---
  // Key: InterlockTestCaseDefinition.case_id
  interlock_test_cases?: { [caseId: string]: InterlockTestCaseExecutionState } | null;
  current_interlock_test_case_id?: string | null; // 当前联锁测试用例ID
  interlock_test_overall_status: ExecutionStatus;

  // --- 未来可能的扩展 ---
  // simulation_parameters?: { [key: string]: any };
  // device_communication_status?: { [deviceId: string]: CommStatus };
} 