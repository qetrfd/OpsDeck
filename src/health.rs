use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;
use std::fmt;
use std::io::Read;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT_SECONDS: u64 = 5;
const CONNECT_TIMEOUT_SECONDS: u64 = 3;
const MAX_BODY_BYTES: u64 = 65_536;
const MAX_PREVIEW_CHARACTERS: usize = 400;
const SLOW_RESPONSE_MILLISECONDS: u128 = 2_000;

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    NotConfigured,
    Healthy,
    Degraded,
    Unhealthy,
    Timeout,
    Unreachable,
    InvalidUrl,
}

impl fmt::Display for HealthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::NotConfigured => "Sin configurar",
            Self::Healthy => "Saludable",
            Self::Degraded => "Degradado",
            Self::Unhealthy => "No saludable",
            Self::Timeout => "Timeout",
            Self::Unreachable => "No disponible",
            Self::InvalidUrl => "URL inválida",
        };

        write!(formatter, "{label}")
    }
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub url: Option<String>,
    pub state: HealthState,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub content_type: Option<String>,
    pub json_valid: Option<bool>,
    pub body_preview: Option<String>,
    pub error: Option<String>,
}

impl HealthCheck {
    pub fn not_configured() -> Self {
        Self {
            url: None,
            state: HealthState::NotConfigured,
            status_code: None,
            latency_ms: None,
            content_type: None,
            json_valid: None,
            body_preview: None,
            error: None,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self.state, HealthState::Healthy | HealthState::Degraded)
    }

    pub fn is_healthy(&self) -> bool {
        self.state == HealthState::Healthy
    }
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self::not_configured()
    }
}

pub fn check_optional_url(url: Option<&str>) -> HealthCheck {
    match url.map(str::trim).filter(|value| !value.is_empty()) {
        Some(url) => check_url(url),
        None => HealthCheck::not_configured(),
    }
}

pub fn check_url(input: &str) -> HealthCheck {
    let input = input.trim();

    let parsed_url = match Url::parse(input) {
        Ok(url) => url,
        Err(error) => {
            return HealthCheck {
                url: Some(input.to_string()),
                state: HealthState::InvalidUrl,
                status_code: None,
                latency_ms: None,
                content_type: None,
                json_valid: None,
                body_preview: None,
                error: Some(format!("No se pudo interpretar la URL: {error}")),
            };
        }
    };

    if !matches!(parsed_url.scheme(), "http" | "https") {
        return HealthCheck {
            url: Some(redacted_url(&parsed_url)),
            state: HealthState::InvalidUrl,
            status_code: None,
            latency_ms: None,
            content_type: None,
            json_valid: None,
            body_preview: None,
            error: Some("La URL debe utilizar http o https".to_string()),
        };
    }

    if parsed_url.host_str().is_none() {
        return HealthCheck {
            url: Some(redacted_url(&parsed_url)),
            state: HealthState::InvalidUrl,
            status_code: None,
            latency_ms: None,
            content_type: None,
            json_valid: None,
            body_preview: None,
            error: Some("La URL no contiene un host válido".to_string()),
        };
    }

    let safe_url = redacted_url(&parsed_url);

    let client = match http_client() {
        Ok(client) => client,
        Err(error) => {
            return HealthCheck {
                url: Some(safe_url),
                state: HealthState::Unreachable,
                status_code: None,
                latency_ms: None,
                content_type: None,
                json_valid: None,
                body_preview: None,
                error: Some(error),
            };
        }
    };

    let started_at = Instant::now();

    let response = match client
        .get(parsed_url)
        .header(ACCEPT, "application/json, text/plain, */*")
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            let latency_ms = started_at.elapsed().as_millis();

            let state = if error.is_timeout() {
                HealthState::Timeout
            } else if error.is_builder() {
                HealthState::InvalidUrl
            } else {
                HealthState::Unreachable
            };

            return HealthCheck {
                url: Some(safe_url),
                state,
                status_code: error.status().map(|status| status.as_u16()),
                latency_ms: Some(latency_ms),
                content_type: None,
                json_valid: None,
                body_preview: None,
                error: Some(error.without_url().to_string()),
            };
        }
    };

    let status = response.status();

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let mut reader = response.take(MAX_BODY_BYTES);
    let mut body = Vec::new();

    let body_error = reader
        .read_to_end(&mut body)
        .err()
        .map(|error| format!("No se pudo leer completamente la respuesta: {error}"));

    let latency_ms = started_at.elapsed().as_millis();
    let body_preview = create_preview(&body);

    let claims_json = content_type
        .as_deref()
        .map(|value| value.to_lowercase().contains("json"))
        .unwrap_or(false)
        || body_looks_like_json(&body);

    let json_valid = if claims_json {
        Some(serde_json::from_slice::<Value>(&body).is_ok())
    } else {
        None
    };

    let mut state = if status.is_success() {
        HealthState::Healthy
    } else {
        HealthState::Unhealthy
    };

    if status.is_success()
        && (latency_ms >= SLOW_RESPONSE_MILLISECONDS
            || json_valid == Some(false)
            || body_error.is_some())
    {
        state = HealthState::Degraded;
    }

    HealthCheck {
        url: Some(safe_url),
        state,
        status_code: Some(status.as_u16()),
        latency_ms: Some(latency_ms),
        content_type,
        json_valid,
        body_preview,
        error: body_error,
    }
}

fn http_client() -> Result<&'static Client, String> {
    match HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
            .user_agent(concat!(
                "OpsDeck/",
                env!("CARGO_PKG_VERSION"),
                " health-monitor"
            ))
            .build()
            .map_err(|error| format!("No se pudo crear el cliente HTTP: {error}"))
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone()),
    }
}

fn body_looks_like_json(body: &[u8]) -> bool {
    let first_character = body
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace());

    matches!(first_character, Some(b'{') | Some(b'['))
}

fn create_preview(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }

    let text = String::from_utf8_lossy(body);

    let preview = text
        .chars()
        .take(MAX_PREVIEW_CHARACTERS)
        .collect::<String>()
        .trim()
        .to_string();

    if preview.is_empty() {
        None
    } else {
        Some(preview)
    }
}

fn redacted_url(url: &Url) -> String {
    let mut safe_url = url.clone();

    let _ = safe_url.set_username("");
    let _ = safe_url.set_password(None);

    safe_url.set_query(None);
    safe_url.set_fragment(None);

    safe_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_url_is_not_configured() {
        let result = check_optional_url(None);

        assert_eq!(result.state, HealthState::NotConfigured);
        assert!(result.url.is_none());
    }

    #[test]
    fn empty_url_is_not_configured() {
        let result = check_optional_url(Some("   "));

        assert_eq!(result.state, HealthState::NotConfigured);
    }

    #[test]
    fn malformed_url_is_invalid() {
        let result = check_url("esto no es una url");

        assert_eq!(result.state, HealthState::InvalidUrl);
        assert!(result.error.is_some());
    }

    #[test]
    fn unsupported_scheme_is_invalid() {
        let result = check_url("file:///tmp/status.json");

        assert_eq!(result.state, HealthState::InvalidUrl);
    }

    #[test]
    fn preview_is_limited() {
        let body = vec![b'a'; 1_000];
        let preview = create_preview(&body).expect("Debe existir una vista previa");

        assert_eq!(preview.chars().count(), MAX_PREVIEW_CHARACTERS);
    }

    #[test]
    fn json_detection_ignores_whitespace() {
        assert!(body_looks_like_json(b"   {\"ok\":true}"));
        assert!(body_looks_like_json(b"\n [1,2,3]"));
        assert!(!body_looks_like_json(b"healthy"));
    }
}
