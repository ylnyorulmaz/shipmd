//! Optional password protection for the web UI, `/api/*` and `/ws`.
//!
//! Set `BOARD_PASSWORD` to enable it. The browser's first request gets an HTTP Basic prompt
//! (any username, that password). A successful login sets an HttpOnly session cookie, which
//! fetch and the WebSocket then send automatically. Scripts can keep using Basic auth:
//! `curl -u :PASSWORD …`.
//!
//! `/mcp` has its own bearer auth (`MCP_API_KEY`) and `/healthz` stays public, so neither goes
//! through this guard.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine;
use sha2::{Digest, Sha256};

const COOKIE: &str = "sb_session";

#[derive(Clone)]
pub struct BoardAuth {
    password: Arc<str>,
    /// Derived from the password, so sessions survive restarts and a password change logs everyone out.
    session_token: Arc<str>,
}

impl BoardAuth {
    pub fn from_env() -> Option<Self> {
        let password = std::env::var("BOARD_PASSWORD").ok().filter(|p| !p.is_empty());
        let Some(password) = password else {
            tracing::warn!("BOARD_PASSWORD not set — web UI and /api are open to anyone who can reach this server");
            return None;
        };
        let digest = Sha256::digest(format!("ship-board-session:v1:{password}"));
        let session_token: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        tracing::info!("board password protection enabled");
        Some(Self { password: password.into(), session_token: session_token.into() })
    }

    fn has_session(&self, headers: &HeaderMap) -> bool {
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(name, value)| name == COOKIE && constant_time_eq(value.as_bytes(), self.session_token.as_bytes()))
    }

    fn has_valid_basic(&self, headers: &HeaderMap) -> bool {
        let Some(encoded) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Basic "))
        else {
            return false;
        };
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
            return false;
        };
        let decoded = String::from_utf8_lossy(&decoded);
        // Any username; only the password matters.
        let password = decoded.split_once(':').map_or(decoded.as_ref(), |(_, p)| p);
        constant_time_eq(password.as_bytes(), self.password.as_bytes())
    }
}

/// Compare secrets without leaking how many leading bytes matched.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn require(State(auth): State<Option<BoardAuth>>, req: Request, next: Next) -> Response {
    let Some(auth) = auth else {
        return next.run(req).await;
    };
    if auth.has_session(req.headers()) {
        return next.run(req).await;
    }
    if !auth.has_valid_basic(req.headers()) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, r#"Basic realm="Ship Board", charset="UTF-8""#)],
            "Password required",
        )
            .into_response();
    }

    // Behind Fly / another TLS proxy, only send the cookie over HTTPS.
    let https = req
        .headers()
        .get("x-forwarded-proto")
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"https"));
    let mut response = next.run(req).await;
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000{}",
        auth.session_token,
        if https { "; Secure" } else { "" }
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> BoardAuth {
        BoardAuth { password: "s3cret".into(), session_token: "tok".into() }
    }

    fn headers(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn basic_auth_checks_password_only() {
        let b64 = |s: &str| base64::engine::general_purpose::STANDARD.encode(s);
        assert!(auth().has_valid_basic(&headers(header::AUTHORIZATION, &format!("Basic {}", b64("me:s3cret")))));
        assert!(auth().has_valid_basic(&headers(header::AUTHORIZATION, &format!("Basic {}", b64(":s3cret")))));
        assert!(!auth().has_valid_basic(&headers(header::AUTHORIZATION, &format!("Basic {}", b64("me:wrong")))));
        assert!(!auth().has_valid_basic(&headers(header::AUTHORIZATION, "Bearer s3cret")));
    }

    #[test]
    fn session_cookie_is_found_among_others() {
        assert!(auth().has_session(&headers(header::COOKIE, "theme=dark; sb_session=tok")));
        assert!(!auth().has_session(&headers(header::COOKIE, "sb_session=nope")));
        assert!(!auth().has_session(&HeaderMap::new()));
    }
}
