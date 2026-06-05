use super::http_boundary::request_header_string_unique;
use super::HttpRequest;
use crate::resources;
use std::collections::{BTreeSet, HashMap};

pub(super) const DEFAULT_MCP_HTTP_SESSION_TTL_MS: u128 = 60 * 60 * 1000;
pub(super) const DEFAULT_MCP_HTTP_SESSION_LIMIT: usize = 1024;
pub(super) const MAX_MCP_HTTP_SESSION_ID_BYTES: usize = 256;

#[derive(Clone, Debug)]
pub(super) struct McpHttpSession {
    pub(super) id: String,
    pub(super) protocol_version: String,
    pub(super) client_name: Option<String>,
    pub(super) client_version: Option<String>,
    pub(super) created_at_ms: u128,
    pub(super) last_seen_at_ms: u128,
    pub(super) expires_at_ms: u128,
    pub(super) initialized: bool,
    seen_request_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum McpHttpSessionErrorKind {
    Missing,
    Invalid,
    Unknown,
    Expired,
    ProtocolMismatch,
    DuplicateRequestId,
}

#[derive(Clone, Debug)]
pub(super) struct McpHttpSessionError {
    pub(super) kind: McpHttpSessionErrorKind,
    pub(super) message: String,
}

impl McpHttpSessionError {
    fn new(kind: McpHttpSessionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(super) fn http_status(&self) -> &'static str {
        match self.kind {
            McpHttpSessionErrorKind::Unknown | McpHttpSessionErrorKind::Expired => "404 Not Found",
            McpHttpSessionErrorKind::Missing
            | McpHttpSessionErrorKind::Invalid
            | McpHttpSessionErrorKind::ProtocolMismatch
            | McpHttpSessionErrorKind::DuplicateRequestId => "400 Bad Request",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct McpHttpSessionSnapshot {
    pub(super) session_count: usize,
    pub(super) max_sessions: usize,
    pub(super) ttl_ms: u128,
    pub(super) pruned_expired_sessions: usize,
    pub(super) oldest_created_at_ms: Option<u128>,
    pub(super) newest_last_seen_at_ms: Option<u128>,
    pub(super) named_client_sessions: usize,
    pub(super) versioned_client_sessions: usize,
    pub(super) mcpace_generated_sessions: usize,
}

#[derive(Debug)]
pub(super) struct McpHttpSessionStore {
    sessions: HashMap<String, McpHttpSession>,
    max_sessions: usize,
    ttl_ms: u128,
    pruned_expired_sessions: usize,
}

impl Default for McpHttpSessionStore {
    fn default() -> Self {
        Self::new(
            DEFAULT_MCP_HTTP_SESSION_LIMIT,
            DEFAULT_MCP_HTTP_SESSION_TTL_MS,
        )
    }
}

impl McpHttpSessionStore {
    pub(super) fn new(max_sessions: usize, ttl_ms: u128) -> Self {
        Self {
            sessions: HashMap::new(),
            max_sessions: max_sessions.max(1),
            ttl_ms: ttl_ms.max(1),
            pruned_expired_sessions: 0,
        }
    }

    pub(super) fn create_or_replace(
        &mut self,
        session_id: String,
        protocol_version: &str,
        client_name: Option<String>,
        client_version: Option<String>,
        initialize_request_id_key: Option<String>,
        now_ms: u128,
    ) -> McpHttpSession {
        self.prune_expired(now_ms);
        if !self.sessions.contains_key(&session_id) {
            self.evict_until_capacity_for_insert();
        }
        let mut seen_request_ids = BTreeSet::new();
        if let Some(id_key) = initialize_request_id_key {
            seen_request_ids.insert(id_key);
        }
        let session = McpHttpSession {
            id: session_id.clone(),
            protocol_version: protocol_version.to_string(),
            client_name,
            client_version,
            created_at_ms: now_ms,
            last_seen_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
            initialized: false,
            seen_request_ids,
        };
        self.sessions.insert(session_id, session.clone());
        session
    }

    pub(super) fn touch_from_request(
        &mut self,
        request: &HttpRequest,
        now_ms: u128,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        let raw_session_id = request_header_string_unique(Some(request), "mcp-session-id")
            .map_err(|message| McpHttpSessionError::new(McpHttpSessionErrorKind::Invalid, message))?
            .ok_or_else(|| {
                McpHttpSessionError::new(
                    McpHttpSessionErrorKind::Missing,
                    "missing required Mcp-Session-Id header after initialize",
                )
            })?;
        let session_id = normalize_mcp_http_session_id(&raw_session_id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Invalid,
                "invalid Mcp-Session-Id header; expected bounded visible ASCII",
            )
        })?;
        let protocol_header = request_header_string_unique(Some(request), "mcp-protocol-version")
            .map_err(|message| {
            McpHttpSessionError::new(McpHttpSessionErrorKind::Invalid, message)
        })?;
        self.touch(&session_id, protocol_header.as_deref(), now_ms)
    }

    pub(super) fn touch(
        &mut self,
        session_id: &str,
        protocol_header: Option<&str>,
        now_ms: u128,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        if let Some(session) = self.sessions.get(session_id) {
            if session.expires_at_ms <= now_ms {
                self.sessions.remove(session_id);
                self.pruned_expired_sessions = self.pruned_expired_sessions.saturating_add(1);
                return Err(McpHttpSessionError::new(
                    McpHttpSessionErrorKind::Expired,
                    "expired MCP HTTP session; initialize again before sending more requests",
                ));
            }
        }

        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Unknown,
                "unknown MCP HTTP session; initialize again before sending more requests",
            )
        })?;

        if let Some(protocol_header) = protocol_header {
            if protocol_header != session.protocol_version {
                return Err(McpHttpSessionError::new(
                    McpHttpSessionErrorKind::ProtocolMismatch,
                    format!(
                        "MCP-Protocol-Version header '{}' does not match initialized session protocol '{}'",
                        protocol_header, session.protocol_version
                    ),
                ));
            }
        }

        session.last_seen_at_ms = now_ms;
        session.expires_at_ms = now_ms.saturating_add(self.ttl_ms);
        Ok(session.clone())
    }

    pub(super) fn mark_initialized_from_request(
        &mut self,
        request: &HttpRequest,
        now_ms: u128,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        let touched = self.touch_from_request(request, now_ms)?;
        let session = self.sessions.get_mut(&touched.id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Unknown,
                "unknown MCP HTTP session; initialize again before sending more requests",
            )
        })?;
        session.initialized = true;
        Ok(session.clone())
    }

    pub(super) fn track_request_id(
        &mut self,
        session_id: &str,
        request_id_key: &str,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Unknown,
                "unknown MCP HTTP session; initialize again before sending more requests",
            )
        })?;
        if !session.seen_request_ids.insert(request_id_key.to_string()) {
            return Err(McpHttpSessionError::new(
                McpHttpSessionErrorKind::DuplicateRequestId,
                "JSON-RPC request id was already used in this MCP HTTP session",
            ));
        }
        Ok(session.clone())
    }

    pub(super) fn close_from_request(
        &mut self,
        request: &HttpRequest,
        now_ms: u128,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        let raw_session_id = request_header_string_unique(Some(request), "mcp-session-id")
            .map_err(|message| McpHttpSessionError::new(McpHttpSessionErrorKind::Invalid, message))?
            .ok_or_else(|| {
                McpHttpSessionError::new(
                    McpHttpSessionErrorKind::Missing,
                    "DELETE /mcp requires Mcp-Session-Id",
                )
            })?;
        let session_id = normalize_mcp_http_session_id(&raw_session_id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Invalid,
                "invalid Mcp-Session-Id header; expected bounded visible ASCII",
            )
        })?;
        self.close(&session_id, now_ms)
    }

    pub(super) fn close(
        &mut self,
        session_id: &str,
        now_ms: u128,
    ) -> Result<McpHttpSession, McpHttpSessionError> {
        if let Some(session) = self.sessions.get(session_id) {
            if session.expires_at_ms <= now_ms {
                self.sessions.remove(session_id);
                self.pruned_expired_sessions = self.pruned_expired_sessions.saturating_add(1);
                return Err(McpHttpSessionError::new(
                    McpHttpSessionErrorKind::Expired,
                    "expired MCP HTTP session; initialize again before sending more requests",
                ));
            }
        }
        self.sessions.remove(session_id).ok_or_else(|| {
            McpHttpSessionError::new(
                McpHttpSessionErrorKind::Unknown,
                "unknown MCP HTTP session; initialize again before sending more requests",
            )
        })
    }

    pub(super) fn snapshot(&mut self, now_ms: u128) -> McpHttpSessionSnapshot {
        self.prune_expired(now_ms);
        McpHttpSessionSnapshot {
            session_count: self.sessions.len(),
            max_sessions: self.max_sessions,
            ttl_ms: self.ttl_ms,
            pruned_expired_sessions: self.pruned_expired_sessions,
            oldest_created_at_ms: self
                .sessions
                .values()
                .map(|session| session.created_at_ms)
                .min(),
            newest_last_seen_at_ms: self
                .sessions
                .values()
                .map(|session| session.last_seen_at_ms)
                .max(),
            named_client_sessions: self
                .sessions
                .values()
                .filter(|session| session.client_name.is_some())
                .count(),
            versioned_client_sessions: self
                .sessions
                .values()
                .filter(|session| session.client_version.is_some())
                .count(),
            mcpace_generated_sessions: self
                .sessions
                .values()
                .filter(|session| session.id.starts_with("mcpace-"))
                .count(),
        }
    }

    fn prune_expired(&mut self, now_ms: u128) {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, session| session.expires_at_ms > now_ms);
        self.pruned_expired_sessions = self
            .pruned_expired_sessions
            .saturating_add(before.saturating_sub(self.sessions.len()));
    }

    fn evict_until_capacity_for_insert(&mut self) {
        while self.sessions.len() >= self.max_sessions {
            let Some(oldest_session_id) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.last_seen_at_ms)
                .map(|(session_id, _)| session_id.clone())
            else {
                break;
            };
            self.sessions.remove(&oldest_session_id);
        }
    }
}

pub(super) fn generated_mcp_http_session_id(
    _request: &HttpRequest,
    _id: &crate::json::JsonValue,
    _protocol: &str,
) -> Result<String, String> {
    let random =
        os_random_hex(16).map_err(|error| format!("OS randomness unavailable: {}", error))?;
    Ok(format!("mcpace-{}", random))
}

pub(super) fn normalize_mcp_http_session_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_MCP_HTTP_SESSION_ID_BYTES
        || trimmed.len() > resources::MAX_HTTP_HEADER_LINE_BYTES
    {
        return None;
    }
    if trimmed.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn os_random_hex(byte_count: usize) -> Result<String, getrandom::Error> {
    let mut bytes = vec![0u8; byte_count];
    getrandom::getrandom(&mut bytes)?;
    Ok(hex_bytes(&bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
