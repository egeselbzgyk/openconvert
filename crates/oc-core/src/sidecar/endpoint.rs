//! Who provides the model server (PHASE 9 detail 3, RT C2/A5.5).

use std::path::PathBuf;

use secrecy::SecretString;

use super::server::{read_key_file, OwnedServer, SidecarError};

/// Where the engine's model requests go.
#[derive(Debug)]
pub enum LlmEndpoint {
    /// `--llm-endpoint`: somebody else's server. The engine spawns nothing.
    External {
        url: String,
        api_key: Option<SecretString>,
    },
    /// A server the engine started, and will stop.
    Owned(OwnedServer),
}

impl LlmEndpoint {
    /// The external endpoint if one was given — reading its key file once — and otherwise whatever
    /// `start` starts. `start` is not called at all when an endpoint is given.
    pub fn choose(
        endpoint: Option<String>,
        key_file: Option<PathBuf>,
        start: impl FnOnce() -> Result<OwnedServer, SidecarError>,
    ) -> Result<Self, SidecarError> {
        match endpoint {
            Some(url) => Ok(Self::External {
                url,
                api_key: key_file.as_deref().map(read_key_file).transpose()?,
            }),
            None => Ok(Self::Owned(start()?)),
        }
    }
}
