//! `LocalSidecar`: the `llama-server` the engine starts, the one the desktop app owns, or one a user
//! runs that the capability probe recognises (D8, D10).
//!
//! It is the OpenAI-compatible client with the two settings only this server is known to honour:
//! the GBNF grammar in the request body, and thinking turned off through the chat template
//! (`chat_template_kwargs: {"enable_thinking": false}`). Whatever it does, gate S checks.

use std::time::Duration;

use super::openai_compatible::{ClientConfig, OpenAiCompatible};
use super::{Constraint, ThinkingControl};
use crate::transport::Transport;

/// The client for a `llama-server` behind `transport`, serving `model_id`.
pub fn local_sidecar<T: Transport>(
    transport: T,
    model_id: String,
    temperature: f32,
    timeout: Duration,
) -> OpenAiCompatible<T> {
    OpenAiCompatible::new(
        transport,
        ClientConfig {
            model_id,
            constraint: Constraint::Gbnf,
            thinking: ThinkingControl::ChatTemplateKwargs,
            temperature,
            timeout,
        },
    )
}
