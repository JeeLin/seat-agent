use std::collections::HashMap;

use seat_agent_core::{Tool, ToolDefinition};
use serde::{Deserialize, Serialize};

/// 工具配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// 工具唯一标识
    pub name: String,

    /// 页面显示名称
    pub display_name: String,

    /// 工具描述（LLM 使用）
    pub description: String,

    /// 分类（info_query/action/transfer/utility）
    pub category: String,

    /// 是否启用
    pub enabled: bool,

    /// 是否涉及敏感操作
    #[serde(default)]
    pub sensitive: bool,

    /// 参数定义（JSON Schema）
    pub parameters: serde_json::Value,

    /// 中间回复
    #[serde(default)]
    pub intermediate_reply: Option<IntermediateReplyConfig>,

    /// 执行失败时的回复话术
    #[serde(default)]
    pub error_reply: Option<String>,
}

/// 中间回复配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateReplyConfig {
    /// 回复文本
    pub text: String,

    /// 音频提示文件路径
    #[serde(default)]
    pub audio_cue: Option<String>,
}

/// 工具配置文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigFile {
    /// 工具列表
    pub tools: Vec<ToolConfig>,
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    configs: Vec<ToolConfig>,
}

impl ToolRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            configs: Vec::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let definition = tool.definition();
        self.tools.insert(definition.name.clone(), tool);
    }

    /// 从 JSON 配置加载
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let config: ToolConfigFile = serde_json::from_str(json)?;
        let mut registry = Self::new();

        for tool_config in config.tools {
            registry.configs.push(tool_config);
        }

        Ok(registry)
    }

    /// 导出为 JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let config = ToolConfigFile {
            tools: self.configs.clone(),
        };
        serde_json::to_string_pretty(&config)
    }

    /// 获取工具
    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// 获取工具配置
    pub fn get_config(&self, name: &str) -> Option<&ToolConfig> {
        self.configs.iter().find(|c| c.name == name)
    }

    /// 列出所有工具配置
    pub fn list_configs(&self) -> &[ToolConfig] {
        &self.configs
    }

    /// 列出已启用的工具配置
    pub fn enabled_configs(&self) -> Vec<&ToolConfig> {
        self.configs.iter().filter(|c| c.enabled).collect()
    }

    /// 获取所有工具定义（用于 LLM 请求）
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// 添加工具配置（不绑定具体实现）
    pub fn add_config(&mut self, config: ToolConfig) {
        self.configs.push(config);
    }

    /// 获取工具数量
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 示例：从 JSON 字符串加载工具配置
pub fn example_tool_config() -> &'static str {
    r#"{
  "tools": [
    {
      "name": "knowledge_search",
      "display_name": "知识库检索",
      "description": "搜索知识库获取答案",
      "category": "info_query",
      "enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "搜索关键词或问题描述"
          }
        },
        "required": ["query"]
      },
      "intermediate_reply": {
        "text": "正在查阅知识库...",
        "audio_cue": "sounds/keyboard_typing.mp3"
      },
      "error_reply": "抱歉，知识库查询暂时不可用"
    },
    {
      "name": "order_query",
      "display_name": "订单查询",
      "description": "查询订单信息（状态、金额、下单时间等）",
      "category": "info_query",
      "enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "order_id": {
            "type": "string",
            "description": "订单号"
          }
        },
        "required": ["order_id"]
      },
      "intermediate_reply": {
        "text": "正在查询订单...",
        "audio_cue": "sounds/keyboard_typing.mp3"
      },
      "error_reply": "抱歉，订单查询暂时不可用"
    },
    {
      "name": "refund_query",
      "display_name": "退款查询",
      "description": "查询退款信息（状态、金额、原因、进度）",
      "category": "info_query",
      "enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "refund_id": {
            "type": "string",
            "description": "退款单号"
          }
        },
        "required": ["refund_id"]
      },
      "intermediate_reply": {
        "text": "正在查询退款...",
        "audio_cue": "sounds/keyboard_typing.mp3"
      },
      "error_reply": "抱歉，退款查询暂时不可用"
    },
    {
      "name": "complaint_query",
      "display_name": "投诉查询",
      "description": "查询投诉处理进度（状态、渠道、进度、责任人）",
      "category": "info_query",
      "enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "complaint_id": {
            "type": "string",
            "description": "投诉单号"
          }
        },
        "required": ["complaint_id"]
      },
      "intermediate_reply": {
        "text": "正在查询投诉进度...",
        "audio_cue": "sounds/keyboard_typing.mp3"
      },
      "error_reply": "抱歉，投诉查询暂时不可用"
    },
    {
      "name": "transfer_to_human",
      "display_name": "转人工客服",
      "description": "转接人工客服",
      "category": "transfer",
      "enabled": true,
      "parameters": {
        "type": "object",
        "properties": {
          "reason": {
            "type": "string",
            "description": "转人工原因（内部记录）"
          },
          "reply": {
            "type": "string",
            "description": "回复给客户的话术（委婉专业）"
          }
        },
        "required": ["reason", "reply"]
      },
      "intermediate_reply": {
        "text": "正在为您转接，请稍候...",
        "audio_cue": "sounds/ringing.mp3"
      },
      "error_reply": "转接暂时不可用"
    }
  ]
}"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use seat_agent_core::ToolDefinition;

    struct DummyTool;

    #[async_trait::async_trait]
    impl seat_agent_core::Tool for DummyTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "dummy".into(),
                description: "A dummy tool".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string" }
                    }
                }),
            }
        }

        async fn execute(&self, _args: serde_json::Value) -> seat_agent_core::Result<String> {
            Ok("done".into())
        }
    }

    struct AnotherTool;

    #[async_trait::async_trait]
    impl seat_agent_core::Tool for AnotherTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "another".into(),
                description: "Another tool".into(),
                parameters: serde_json::json!({ "type": "object" }),
            }
        }

        async fn execute(&self, _args: serde_json::Value) -> seat_agent_core::Result<String> {
            Ok("another".into())
        }
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_default_is_empty() {
        let registry = ToolRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_and_get_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        let tool = registry.get_tool("dummy");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().definition().name, "dummy");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let registry = ToolRegistry::new();
        assert!(registry.get_tool("nope").is_none());
    }

    #[test]
    fn test_register_multiple_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        registry.register(Box::new(AnotherTool));
        assert_eq!(registry.len(), 2);

        assert!(registry.get_tool("dummy").is_some());
        assert!(registry.get_tool("another").is_some());
    }

    #[test]
    fn test_register_duplicate_overwrites() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        assert_eq!(registry.len(), 1);

        // Register another tool with the same name "dummy"
        struct DummyToolV2;
        #[async_trait::async_trait]
        impl seat_agent_core::Tool for DummyToolV2 {
            fn definition(&self) -> ToolDefinition {
                ToolDefinition {
                    name: "dummy".into(),
                    description: "V2".into(),
                    parameters: serde_json::json!({ "type": "object" }),
                }
            }
            async fn execute(&self, _: serde_json::Value) -> seat_agent_core::Result<String> {
                Ok("v2".into())
            }
        }
        registry.register(Box::new(DummyToolV2));
        assert_eq!(registry.len(), 1);
        // The second registration should overwrite
        let tool = registry.get_tool("dummy").unwrap();
        assert_eq!(tool.definition().description, "V2");
    }

    #[test]
    fn test_tool_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        registry.register(Box::new(AnotherTool));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 2);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"dummy"));
        assert!(names.contains(&"another"));
    }

    #[test]
    fn test_from_json_loads_config() {
        let json = r#"{
            "tools": [
                {
                    "name": "test_tool",
                    "display_name": "Test",
                    "description": "A test tool",
                    "category": "info_query",
                    "enabled": true,
                    "parameters": { "type": "object" }
                }
            ]
        }"#;

        let registry = ToolRegistry::from_json(json).unwrap();
        assert_eq!(registry.list_configs().len(), 1);
        assert_eq!(registry.list_configs()[0].name, "test_tool");
    }

    #[test]
    fn test_from_json_invalid_returns_error() {
        let result = ToolRegistry::from_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_example_tool_config_valid_json() {
        let json = example_tool_config();
        let result: Result<ToolConfigFile, _> = serde_json::from_str(json);
        assert!(result.is_ok(), "example_tool_config() should be valid JSON");
        let config = result.unwrap();
        assert!(config.tools.len() >= 4, "Expected at least 4 tools");
    }

    #[test]
    fn test_add_config_and_list() {
        let mut registry = ToolRegistry::new();
        let config = ToolConfig {
            name: "my_tool".into(),
            display_name: "My Tool".into(),
            description: "desc".into(),
            category: "info_query".into(),
            enabled: true,
            sensitive: false,
            parameters: serde_json::json!({ "type": "object" }),
            intermediate_reply: None,
            error_reply: None,
        };
        registry.add_config(config);
        assert_eq!(registry.list_configs().len(), 1);
        assert_eq!(registry.list_configs()[0].name, "my_tool");
    }

    #[test]
    fn test_enabled_configs() {
        let mut registry = ToolRegistry::new();
        registry.add_config(ToolConfig {
            name: "on".into(),
            display_name: "On".into(),
            description: "".into(),
            category: "".into(),
            enabled: true,
            sensitive: false,
            parameters: serde_json::json!({ "type": "object" }),
            intermediate_reply: None,
            error_reply: None,
        });
        registry.add_config(ToolConfig {
            name: "off".into(),
            display_name: "Off".into(),
            description: "".into(),
            category: "".into(),
            enabled: false,
            sensitive: false,
            parameters: serde_json::json!({ "type": "object" }),
            intermediate_reply: None,
            error_reply: None,
        });

        let enabled = registry.enabled_configs();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "on");
    }
}
