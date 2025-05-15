use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value; // 引入 serde_json::Value

// TODO: 后续步骤将添加其他结构体定义

/// 模板元数据，包含模板的通用信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TemplateMetadata {
    /// 模板唯一标识符，例如UUID。
    pub template_id: String,
    /// 模板名称。
    pub template_name: String,
    /// 模板版本，例如 "1.0.0"。
    pub template_version: String,
    /// 模板类型。
    pub template_type: TemplateType,
    /// 模板描述，可选。
    pub description: Option<String>,
    /// 模板适用范围，可选，例如 "通用泵类", "XX项目冷却水系统"。
    pub applicable_scope: Option<String>,
    /// 创建者信息，可选，可以是用户ID或名称。
    pub created_by: Option<String>,
    /// 创建时间戳 (UTC)。
    pub created_at: DateTime<Utc>,
    /// 最后更新时间戳 (UTC)。
    pub updated_at: DateTime<Utc>,
}

/// 调试模板的类型枚举。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TemplateType {
    /// 预检查模板。
    PreCheck,
    /// 单体设备测试模板。
    SingleDeviceTest,
    /// 联锁测试模板。
    InterlockTest,
}

/// 字段输入类型枚举，用于定义预检查项或测试步骤中输入字段的类型。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FieldInputType {
    ///布尔类型输入 (例如，是/否，通过/不通过)。
    Boolean,
    /// 数值类型输入。
    Numeric {
        /// 单位，可选 (例如 "℃", "kPa", "rpm")。
        unit: Option<String>,
        /// 最小值限制，可选。
        min: Option<f64>,
        /// 最大值限制，可选。
        max: Option<f64>,
    },
    /// 文本类型输入。
    Text {
        /// 最大长度限制，可选。
        max_length: Option<u32>,
    },
    /// 单选类型输入。
    SingleChoice {
        /// 可选项列表。
        options: Vec<String>,
    },
    /// 多选类型输入。
    MultipleChoice {
        /// 可选项列表。
        options: Vec<String>,
    },
    /// 照片上传类型。
    PhotoUpload {
        /// 是否必须上传。
        required: bool,
        /// 最大照片数量，可选。
        max_photos: Option<u8>,
    },
}

/// 预检查模板定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PreCheckTemplate {
    /// 模板元数据。
    pub metadata: TemplateMetadata,
    /// 预检查项定义列表。
    pub items: Vec<PreCheckItemDefinition>,
}

/// 预检查项定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PreCheckItemDefinition {
    /// 预检查项在其所属模板内的唯一标识。
    pub item_id: String,
    /// 预检查项的显示顺序。
    pub item_order: u32,
    /// 预检查项的类别，用于分组显示。
    pub category: String,
    /// 预检查项的详细描述。
    pub description: String,
    /// 预检查的标准或期望值描述。
    pub standard_or_expected_value: String,
    /// 检查方法提示，可选。
    pub check_method_hint: Option<String>,
    /// 此检查项对应的输入控件类型。
    pub input_type: FieldInputType,
    /// 是否为关键检查项。
    pub is_critical: bool,
    /// 加载时默认显示的状态，例如 "待检查"。
    pub default_status_on_load: String, // 例如 "PENDING_CHECK"
}

/// 点位I/O类型，用于指示点位在模板中的作用。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum PointIoType {
    /// 指令类点位，用于下发控制命令。
    Command,
    /// 反馈类点位，用于读取设备状态或操作结果。
    Feedback,
    /// 参数类点位，用于设定指令的参数值。
    Parameter,
    /// 条件类点位，用于联锁测试中的前置条件或触发条件检查。
    Condition,
    /// 结果类点位，用于联锁测试中验证预期结果。
    Outcome,
}

/// 点位I/O定义，描述模板中引用的一个具体点位及其相关信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PointIoDefinition {
    /// 点位的逻辑名称，应与点表中的定义对应。
    pub point_name: String,
    /// 对此点位在当前模板上下文中的描述，可选。
    pub description: Option<String>,
    /// 点位在此模板上下文中的I/O类型。
    pub io_type: PointIoType,
    /// 点位数据类型的提示，可选，例如 "bool", "f32", "String"。
    pub data_type_hint: Option<String>,
}

/// 单体设备测试模板定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleDeviceTestTemplate {
    /// 模板元数据。
    pub metadata: TemplateMetadata,
    /// 适用的设备类型ID，例如 "VALVE_DN100_PN16", "MOTOR_ABB_XYZ"。
    pub device_type_id: String,
    /// 测试步骤定义列表。
    pub steps: Vec<SingleDeviceTestStepDefinition>,
}

/// 单体设备测试步骤定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleDeviceTestStepDefinition {
    /// 测试步骤在其所属模板内的唯一标识。
    pub step_id: String,
    /// 测试步骤的执行顺序。
    pub step_order: u32,
    /// 测试步骤的名称。
    pub step_name: String,
    /// 测试步骤的详细描述。
    pub description: String,
    /// 指令动作的枚举或标识符，例如 "CMD_OPEN_VALVE", "CMD_SET_SPEED"。
    /// 具体值将由实际的指令系统定义。
    pub command_action_enum: String,
    /// 指令参数的JSON Schema，用于定义和校验指令参数，可选。
    /// 可以是 `serde_json::Value` (通常是Object类型，代表Schema) 或 JSON字符串。
    pub command_parameters_schema: Option<Value>,
    /// 执行指令的目标点位列表，可选。
    pub command_target_points: Option<Vec<PointIoDefinition>>,
    /// 需要向现场操作员展示的反馈提示信息。
    pub feedback_prompt_for_site: String,
    /// 需要读取以获取反馈数据的点位列表。
    pub feedback_points_to_read: Vec<PointIoDefinition>,
    /// 现场反馈的输入数据的JSON Schema，可选。
    /// 用于指导现场如何输入反馈，以及云端如何校验。
    pub feedback_input_schema: Option<Value>,
    /// 测试步骤成功的判断逻辑，使用 `serde_json::Value` 存储，例如一个简单的表达式或规则引擎的JSON配置。
    pub success_criteria_logic: Value,
    /// 测试步骤的超时时间（秒），可选。
    pub timeout_seconds: Option<u32>,
}

/// 联锁测试模板定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InterlockTestTemplate {
    /// 模板元数据。
    pub metadata: TemplateMetadata,
    /// 适用的系统或子系统ID。
    pub system_or_subsystem_id: String,
    /// 联锁测试用例定义列表。
    pub cases: Vec<InterlockTestCaseDefinition>,
}

/// 联锁测试用例定义。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InterlockTestCaseDefinition {
    /// 测试用例在其所属模板内的唯一标识。
    pub case_id: String,
    /// 测试用例的执行顺序。
    pub case_order: u32,
    /// 测试用例的名称。
    pub case_name: String,
    /// 测试用例的详细描述。
    pub description: String,
    /// 前置条件的文字描述。
    pub preconditions_description: String,
    /// 前置条件中需要设置的点位列表。
    pub precondition_points_setup: Vec<ConditionPointSetup>,
    /// 触发动作的文字描述。
    pub trigger_action_description: String,
    /// 触发动作的具体细节，可选，例如需要写入值的点位。
    pub trigger_action_details: Option<TriggerActionDefinition>,
    /// 预期结果的文字描述。
    pub expected_outcome_description: String,
    /// 预期结果中需要检查的点位列表。
    pub expected_outcome_points_check: Vec<ExpectedPointOutcome>,
    /// 测试用例成功的判断逻辑，使用 `serde_json::Value` 存储。
    pub success_criteria_logic: Value,
    /// 测试用例的超时时间（秒），可选。
    pub timeout_seconds: Option<u32>,
}

/// 用于联锁测试前置条件设置的点位信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConditionPointSetup {
    /// 目标点位的逻辑名称。
    pub point_name: String,
    /// 目标值的文字描述 (例如 "打开", "设定为50%")。
    /// 实际的目标值可能在执行时动态确定或从变量获取。
    pub target_value_description: String,
    /// 点位设置方法的提示，可选 (例如 "手动操作阀门X至全开位置", "通过HMI设置参数Y")。
    pub setup_method_hint: Option<String>,
}

/// 触发动作的详细定义，通常包含需要写入值的点位。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TriggerActionDefinition {
    /// 执行触发动作时，需要写入值的目标点位列表，可选。
    pub command_target_points: Option<Vec<PointReferenceWithValue>>,
}

/// 表示一个点位引用及其需要写入的值的来源。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PointReferenceWithValue {
    /// 目标点位的逻辑名称。
    pub point_name: String,
    /// 需要写入点位的值的来源。
    pub value_to_write_source: ValueSource,
}

/// 定义一个值的来源类型。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ValueSource {
    /// 值是一个字面量，直接提供。
    Literal(Value),
    /// 值来源于某个参数的名称，执行时动态获取。
    FromParameter(String),
    /// 值来源于前一个步骤的输出，执行时动态获取。
    FromPreviousStepOutput(String),
}

/// 联锁测试中用于定义预期结果的点位检查信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExpectedPointOutcome {
    /// 需要检查状态或值的点位的逻辑名称。
    pub point_name: String,
    /// 预期值的文字描述 (例如 "应关闭", "应大于100")。
    /// 实际的预期值可能在执行时动态确定或从变量获取。
    pub expected_value_description: String,
    /// 比较操作符，可选 (例如 "==", ">", "<=", "InRange")。
    /// 如果为None，可能表示仅关注状态变化或由 `success_criteria_logic` 处理复杂比较。
    pub comparison_operator: Option<String>,
} 