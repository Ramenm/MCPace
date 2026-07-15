use std::fmt;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HttpClientError {
    Transport { reason: String },
    Read { reason: String },
    Status { status: u16 },
}

pub(crate) type HttpClientResult<T> = Result<T, HttpClientError>;

impl fmt::Display for HttpClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { reason } => write!(formatter, "HTTPS request failed: {}", reason),
            Self::Read { reason } => write!(formatter, "failed to read HTTPS response: {}", reason),
            Self::Status { status } => {
                write!(formatter, "HTTPS request returned status {}", status)
            }
        }
    }
}

impl std::error::Error for HttpClientError {}

pub(crate) fn bounded_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        // MCP requests can carry credentials. Never forward them through an
        // endpoint-controlled redirect; callers should configure the final URL.
        .max_redirects(0)
        .http_status_as_error(false)
        // Use the machine's trust policy so corporate/user-installed roots work
        // consistently on Windows, macOS, and Linux.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

pub(crate) fn bounded_get_text(
    url: &str,
    timeout: Duration,
    max_response_bytes: usize,
) -> HttpClientResult<String> {
    let agent = bounded_agent(timeout);
    let mut response = agent
        .get(url)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| HttpClientError::Transport {
            reason: error.to_string(),
        })?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(max_response_bytes as u64)
        .read_to_string()
        .map_err(|error| HttpClientError::Read {
            reason: error.to_string(),
        })?;
    if !(200..300).contains(&status) {
        return Err(HttpClientError::Status { status });
    }
    Ok(body)
}
