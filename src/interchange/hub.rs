use serde::{Deserialize, Serialize};

pub const UCF_VERSION: &str = "1.0.0";

/// A single record in a .ucf.jsonl file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HubRecord {
    #[serde(rename = "session")]
    Session(SessionHeader),
    #[serde(rename = "message")]
    Message(HubMessage),
    #[serde(rename = "event")]
    Event(HubEvent),
}

/// First line of a .ucf.jsonl file — session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub ucf_version: String,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_cli: String,
    pub source_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub directory: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_url: Option<String>,
}

/// A conversation message (user, assistant, or system).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubMessage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub role: String,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub metadata: MessageMetadata,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
    #[serde(default)]
    pub tokens_cumulative: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_creation: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub tool: u64,
    #[serde(default)]
    pub total: u64,
}

/// Content blocks within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentBlock>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        interrupted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        truncated: bool,
    },

    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        #[serde(default)]
        encrypted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encryption_format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encrypted_data: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    #[serde(rename = "image")]
    Image {
        media_type: String,
        encoding: String,
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_url: Option<String>,
    },

    #[serde(rename = "step_boundary")]
    StepBoundary {
        boundary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
    },

    #[serde(rename = "patch")]
    Patch {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hash_before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hash_after: Option<String>,
    },
}

/// Non-message events (hooks, metadata changes, lifecycle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubEvent {
    pub event_type: String,
    pub timestamp: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hub_record_serialization() {
        let session = HubRecord::Session(SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "test-session".to_string(),
            created_at: "2026-03-29T12:00:00Z".to_string(),
            updated_at: "2026-03-29T12:30:00Z".to_string(),
            source_cli: "claude-code".to_string(),
            source_version: "2.1.87".to_string(),
            project: None,
            model: Some("claude-opus-4-6".to_string()),
            title: Some("Test session".to_string()),
            slug: None,
            parent_session_id: None,
            extensions: serde_json::Value::Null,
        });

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"type\":\"session\""));
        let parsed: HubRecord = serde_json::from_str(&json).unwrap();
        match parsed {
            HubRecord::Session(s) => assert_eq!(s.session_id, "test-session"),
            _ => panic!("Expected Session variant"),
        }
    }

    #[test]
    fn test_content_block_variants() {
        let blocks = vec![
            ContentBlock::Text {
                text: "hello".into(),
            },
            ContentBlock::ToolUse {
                id: "tool-1".into(),
                name: "bash".into(),
                display_name: None,
                description: None,
                input: serde_json::json!({"command": "ls"}),
            },
            ContentBlock::Thinking {
                text: "hmm".into(),
                subject: None,
                description: None,
                signature: Some("sig123".into()),
                encrypted: false,
                encryption_format: None,
                encrypted_data: None,
                timestamp: None,
            },
            ContentBlock::StepBoundary {
                boundary: "start".into(),
                snapshot: Some("abc123".into()),
                finish_reason: None,
                cost: None,
                tokens: None,
            },
            ContentBlock::Patch {
                path: "/src/main.rs".into(),
                hash_before: Some("aaa".into()),
                hash_after: Some("bbb".into()),
            },
        ];

        for block in &blocks {
            let json = serde_json::to_string(block).unwrap();
            let _parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_tool_result_with_array_content() {
        let result = ContentBlock::ToolResult {
            tool_use_id: "tool-1".into(),
            content: vec![
                ContentBlock::Text {
                    text: "output text".into(),
                },
                ContentBlock::Image {
                    media_type: "image/png".into(),
                    encoding: "base64".into(),
                    data: "abc123".into(),
                    source_url: None,
                },
            ],
            exit_code: Some(0),
            is_error: false,
            interrupted: false,
            status: Some("completed".into()),
            duration_ms: Some(150),
            title: None,
            truncated: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        if let ContentBlock::ToolResult { content, .. } = parsed {
            assert_eq!(content.len(), 2);
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_message_round_trip() {
        let msg = HubRecord::Message(HubMessage {
            id: "msg-1".into(),
            api_message_id: Some("msg_01ABC".into()),
            parent_id: None,
            timestamp: "2026-03-29T12:00:00Z".into(),
            completed_at: Some("2026-03-29T12:00:03Z".into()),
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            metadata: MessageMetadata {
                model: Some("claude-opus-4-6".into()),
                tokens: Some(TokenUsage {
                    input: 100,
                    output: 50,
                    cache_creation: 500,
                    cache_read: 200,
                    reasoning: 0,
                    tool: 0,
                    total: 850,
                }),
                ..Default::default()
            },
            extensions: serde_json::json!({"claude-code": {"isSidechain": false}}),
        });

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: HubRecord = serde_json::from_str(&json).unwrap();
        match parsed {
            HubRecord::Message(m) => {
                assert_eq!(m.api_message_id, Some("msg_01ABC".into()));
                assert_eq!(m.completed_at, Some("2026-03-29T12:00:03Z".into()));
                let tokens = m.metadata.tokens.unwrap();
                assert_eq!(tokens.cache_creation, 500);
                assert_eq!(tokens.cache_read, 200);
            }
            _ => panic!("Expected Message"),
        }
    }
}
