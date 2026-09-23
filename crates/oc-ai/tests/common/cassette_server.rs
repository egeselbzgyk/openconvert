//! A stand-in server that answers from the committed cassettes, in whichever wire format it is
//! asked in — OpenAI's `/v1/chat/completions` or Ollama's `/api/chat` (PHASE 11, rows 11.3 and
//! 11.10).
//!
//! It is a `Transport`, not a socket: the adapter under test builds its real request body, the
//! server reads the question out of that body the way a server would, finds the recording of that
//! exact question, and replies in the shape that adapter's server replies in. So what passes through
//! it is the adapter's own wire format end to end, and the answer is the cassette's.
//!
//! **Matching is exact.** The system message is the shared prefix (with `/no_think` appended for a
//! generic endpoint, D10) and the user message is the recorded question — or the recorded question
//! followed by a blank line and that task's own `schema.json`, which is what an adapter that
//! constrains nothing sends (PHASE 11 detail 1). Anything else is a 404, never a nearest match.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use oc_ai::cassette::Cassette;
use oc_ai::digest::{hex, sha256};
use oc_ai::prompt;
use oc_ai::provider::Purpose;
use oc_ai::transport::{Transport, TransportError};
use serde_json::{json, Value};

pub const OPENAI_PATH: &str = "/v1/chat/completions";
pub const OLLAMA_PATH: &str = "/api/chat";

/// What the server was asked: the path, and the body as JSON.
#[derive(Clone, Debug)]
pub struct Asked {
    pub path: String,
    pub body: Value,
}

pub struct CassetteServer {
    cassettes: Vec<Cassette>,
    asked: Mutex<Vec<Asked>>,
}

impl CassetteServer {
    /// Every cassette under `root/<task>/`.
    pub fn load(root: &Path) -> Self {
        let mut cassettes = Vec::new();
        for purpose in Purpose::ALL {
            let directory = root.join(purpose.as_str());
            let mut files: Vec<_> = std::fs::read_dir(&directory)
                .expect("a task directory")
                .map(|entry| entry.expect("an entry").path())
                .filter(|path| path.file_name().is_some_and(|name| name != "index.json"))
                .collect();
            files.sort();
            for file in files {
                let text = std::fs::read_to_string(&file).expect("a cassette");
                cassettes.push(serde_json::from_str(&text).expect("a cassette"));
            }
        }
        Self {
            cassettes,
            asked: Mutex::new(Vec::new()),
        }
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().expect("not poisoned").clone()
    }

    pub fn last(&self) -> Asked {
        self.asked().pop().expect("the server was asked something")
    }

    /// The recording of the question this body asks, if there is one.
    fn find(&self, body: &Value) -> Option<&Cassette> {
        let system = body["messages"][0]["content"].as_str()?;
        let user = body["messages"][1]["content"].as_str()?;
        let system = system.strip_suffix("/no_think").unwrap_or(system);
        self.cassettes.iter().find(|cassette| {
            if cassette.request.system_sha256 != hex(&sha256(system.as_bytes())) {
                return false;
            }
            let question = cassette.request.user.as_str();
            if user == question {
                return true;
            }
            let Some(purpose) = Purpose::ALL
                .into_iter()
                .find(|purpose| purpose.as_str() == cassette.task)
            else {
                return false;
            };
            let schema = prompt::artifacts(purpose).schema.trim_end();
            user.strip_prefix(question)
                .and_then(|rest| rest.strip_prefix("\n\n"))
                .is_some_and(|rest| rest == schema)
        })
    }
}

impl Transport for &CassetteServer {
    fn post_json(&self, path: &str, body: &str, _: Duration) -> Result<String, TransportError> {
        let body: Value = serde_json::from_str(body).expect("the adapter sends JSON");
        self.asked.lock().expect("not poisoned").push(Asked {
            path: path.to_owned(),
            body: body.clone(),
        });
        let Some(cassette) = self.find(&body) else {
            return Err(TransportError::Status { status: 404 });
        };
        let response = &cassette.response;
        match path {
            OPENAI_PATH => Ok(json!({
                "id": "chatcmpl-cassette",
                "object": "chat.completion",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": response.content,
                        "reasoning_content": response.reasoning,
                    },
                    "finish_reason": response.finish_reason,
                }],
                "usage": {
                    "prompt_tokens": response.tokens_in,
                    "completion_tokens": response.tokens_out,
                },
            })
            .to_string()),
            OLLAMA_PATH => {
                let mut message = json!({ "role": "assistant", "content": response.content });
                if let Some(thinking) = &response.reasoning {
                    message["thinking"] = Value::from(thinking.as_str());
                }
                Ok(json!({
                    "model": body["model"],
                    "created_at": "2026-09-23T00:00:00Z",
                    "message": message,
                    "done": true,
                    "done_reason": response.finish_reason,
                    "prompt_eval_count": response.tokens_in,
                    "eval_count": response.tokens_out,
                })
                .to_string())
            }
            _ => Err(TransportError::Status { status: 404 }),
        }
    }
}
