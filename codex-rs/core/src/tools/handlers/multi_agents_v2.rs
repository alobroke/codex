//! Implements the MultiAgentV2 collaboration tool surface.

use crate::agent::AgentStatus;
use crate::agent::agent_resolver::resolve_agent_target;
use crate::agent::types::AgentMessage;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::multi_agents_common::*;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_protocol::items::CollabAgentTool;
use codex_protocol::items::CollabAgentToolCallItem;
use codex_protocol::items::CollabAgentToolCallStatus;
use codex_protocol::items::SubAgentActivityItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::SubAgentActivityKind;
use codex_tools::ToolName;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;

pub(crate) use followup_task::Handler as FollowupTaskHandler;
pub(crate) use interrupt_agent::Handler as InterruptAgentHandler;
pub(crate) use list_agents::Handler as ListAgentsHandler;
pub(crate) use send_message::Handler as SendMessageHandler;
pub(crate) use spawn::Handler as SpawnAgentHandler;
pub(crate) use wait::Handler as WaitAgentHandler;

mod analytics;
mod followup_task;
mod interrupt_agent;
mod list_agents;
mod message_tool;
mod send_message;
mod spawn;
pub(crate) mod wait;

pub(crate) async fn emit_sub_agent_activity(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
    item: SubAgentActivityItem,
) {
    let item = TurnItem::SubAgentActivity(item);
    session.emit_turn_item_started(turn, &item).await;
    session.emit_turn_item_completed(turn, item).await;
}

fn agent_message_from_tool(
    message: String,
    source: &crate::tools::context::ToolCallSource,
) -> AgentMessage {
    if matches!(
        source,
        crate::tools::context::ToolCallSource::DirectPlaintextMessage
            | crate::tools::context::ToolCallSource::CodeMode { .. }
    ) {
        AgentMessage::Plaintext(message)
    } else {
        AgentMessage::Encrypted(message)
    }
}

fn message_for_activity(message: &AgentMessage) -> Option<String> {
    match message {
        AgentMessage::Plaintext(message) => Some(message.clone()),
        AgentMessage::Encrypted(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_message_is_included_in_activity() {
        let message = AgentMessage::Plaintext("Analyze the authentication flow".to_string());

        assert_eq!(
            message_for_activity(&message),
            Some("Analyze the authentication flow".to_string())
        );
    }

    #[test]
    fn encrypted_message_is_not_included_in_activity() {
        let message = AgentMessage::Encrypted("encrypted-payload".to_string());

        assert_eq!(message_for_activity(&message), None);
    }

    #[test]
    fn direct_tool_message_is_encrypted() {
        let message = agent_message_from_tool(
            "sensitive payload".to_string(),
            &crate::tools::context::ToolCallSource::Direct,
        );

        assert_eq!(
            message,
            AgentMessage::Encrypted("sensitive payload".to_string())
        );
    }

    #[test]
    fn direct_plaintext_tool_message_is_plaintext() {
        let message = agent_message_from_tool(
            "Analyze the authentication flow".to_string(),
            &crate::tools::context::ToolCallSource::DirectPlaintextMessage,
        );

        assert_eq!(
            message,
            AgentMessage::Plaintext("Analyze the authentication flow".to_string())
        );
    }

    #[test]
    fn code_mode_tool_message_is_plaintext() {
        let message = agent_message_from_tool(
            "Inspect the API routes".to_string(),
            &crate::tools::context::ToolCallSource::CodeMode {
                cell_id: "cell-1".to_string(),
                runtime_tool_call_id: "call-1".to_string(),
            },
        );

        assert_eq!(
            message,
            AgentMessage::Plaintext("Inspect the API routes".to_string())
        );
    }
}
