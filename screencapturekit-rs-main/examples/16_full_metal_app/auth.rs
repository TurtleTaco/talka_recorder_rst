//! Auth0 Device Authorization Flow
//!
//! Implements OAuth 2.0 Device Authorization Grant (RFC 8628)
//! https://auth0.com/docs/get-started/authentication-and-authorization-flow/device-authorization-flow

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const AUTH0_DOMAIN: &str = "login.talka.ai";
const CLIENT_ID: &str = "ZTQTqV6jnRjRFPPQlVbITW6L5FkM4jB8";
const CLIENT_SECRET: &str = "d4AkZz2BagYrEO38QoSwkMJOFp_e75DpTykVkdeOujKqsgcbT0-_1qbgX-schvpu";
const AUDIENCE: &str = "https://talka/api";

#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    grant_type: String,
    device_code: String,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success {
        access_token: String,
        #[serde(default)]
        refresh_token: String,
        #[serde(default)]
        id_token: String,
        token_type: String,
        expires_in: u64,
    },
    Error {
        error: String,
        error_description: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub expires_at: u64, // Unix timestamp when token expires
}

impl AuthTokens {
    /// Check if the access token is expired or will expire in the next 5 minutes
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Consider expired if less than 5 minutes remaining
        self.expires_at.saturating_sub(now) < 300
    }

    /// Update expiration timestamp based on expires_in
    pub fn update_expiration(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.expires_at = now + self.expires_in;
    }
}

#[derive(Debug, Serialize)]
struct RefreshTokenRequest {
    grant_type: String,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}

#[derive(Debug)]
pub enum AuthError {
    NetworkError(String),
    AuthorizationPending,
    SlowDown,
    AccessDenied,
    ExpiredToken,
    InvalidRequest(String),
    Unknown(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
            Self::AuthorizationPending => write!(f, "Authorization pending"),
            Self::SlowDown => write!(f, "Polling too frequently"),
            Self::AccessDenied => write!(f, "Access denied by user"),
            Self::ExpiredToken => write!(f, "Device code expired"),
            Self::InvalidRequest(msg) => write!(f, "Invalid request: {msg}"),
            Self::Unknown(msg) => write!(f, "Unknown error: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Starts the device authorization flow
///
/// Returns the verification URI and user code that should be displayed to the user
pub async fn start_device_flow() -> Result<(String, String, DeviceCodeResponse), AuthError> {
    let client = reqwest::Client::new();
    let url = format!("https://{}/oauth/device/code", AUTH0_DOMAIN);

    let request = DeviceCodeRequest {
        client_id: CLIENT_ID.to_string(),
        audience: Some(AUDIENCE.to_string()),
        scope: Some("openid profile email offline_access".to_string()),
    };

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&request)
        .send()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AuthError::NetworkError(format!(
            "HTTP {}: {}",
            status, text
        )));
    }

    let device_response: DeviceCodeResponse = response
        .json()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    Ok((
        device_response.verification_uri.clone(),
        device_response.user_code.clone(),
        device_response,
    ))
}

/// Polls for the access token
///
/// This should be called repeatedly (respecting the interval) until the user completes
/// authentication or the device code expires
pub async fn poll_for_token(device_code: &str) -> Result<AuthTokens, AuthError> {
    let client = reqwest::Client::new();
    let url = format!("https://{}/oauth/token", AUTH0_DOMAIN);

    let request = TokenRequest {
        grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        device_code: device_code.to_string(),
        client_id: CLIENT_ID.to_string(),
        client_secret: Some(CLIENT_SECRET.to_string()),
    };

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&request)
        .send()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    match token_response {
        TokenResponse::Success {
            access_token,
            refresh_token,
            id_token,
            token_type,
            expires_in,
        } => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(AuthTokens {
                access_token,
                refresh_token,
                id_token,
                token_type,
                expires_in,
                expires_at: now + expires_in,
            })
        }
        TokenResponse::Error {
            error,
            error_description,
        } => match error.as_str() {
            "authorization_pending" => Err(AuthError::AuthorizationPending),
            "slow_down" => Err(AuthError::SlowDown),
            "access_denied" => Err(AuthError::AccessDenied),
            "expired_token" => Err(AuthError::ExpiredToken),
            _ => Err(AuthError::Unknown(
                error_description.unwrap_or_else(|| error.clone()),
            )),
        },
    }
}

/// Refresh an access token using a refresh token
pub async fn refresh_access_token(refresh_token: &str) -> Result<AuthTokens, AuthError> {
    let client = reqwest::Client::new();
    let url = format!("https://{}/oauth/token", AUTH0_DOMAIN);

    let request = RefreshTokenRequest {
        grant_type: "refresh_token".to_string(),
        client_id: CLIENT_ID.to_string(),
        client_secret: CLIENT_SECRET.to_string(),
        refresh_token: refresh_token.to_string(),
    };

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&request)
        .send()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    match token_response {
        TokenResponse::Success {
            access_token,
            refresh_token: new_refresh_token,
            id_token,
            token_type,
            expires_in,
        } => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            // Use new refresh token if provided, otherwise keep the old one
            let final_refresh_token = if new_refresh_token.is_empty() {
                refresh_token.to_string()
            } else {
                new_refresh_token
            };
            
            Ok(AuthTokens {
                access_token,
                refresh_token: final_refresh_token,
                id_token,
                token_type,
                expires_in,
                expires_at: now + expires_in,
            })
        }
        TokenResponse::Error {
            error,
            error_description,
        } => Err(AuthError::Unknown(
            error_description.unwrap_or_else(|| error.clone()),
        )),
    }
}

/// Get the path to the token storage file
fn get_token_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".talka_tokens.json")
}

/// Save tokens to disk
pub fn save_tokens(tokens: &AuthTokens) -> Result<(), std::io::Error> {
    let path = get_token_file_path();
    let json = serde_json::to_string_pretty(tokens)?;
    fs::write(&path, json)?;
    Ok(())
}

/// Load tokens from disk
pub fn load_tokens() -> Option<AuthTokens> {
    let path = get_token_file_path();
    if !path.exists() {
        return None;
    }

    match fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str::<AuthTokens>(&json) {
            Ok(tokens) => Some(tokens),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

/// Logout - delete stored tokens
pub fn logout() -> Result<(), std::io::Error> {
    let path = get_token_file_path();
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Get valid tokens - either from cache or by authenticating
///
/// This function:
/// 1. Checks for existing tokens on disk
/// 2. If found and valid, returns them
/// 3. If expired but has refresh token, refreshes them
/// 4. Otherwise, starts new device flow
pub async fn get_valid_tokens() -> Result<AuthTokens, AuthError> {
    // Try to load existing tokens
    if let Some(tokens) = load_tokens() {
        if tokens.is_expired() {
            if !tokens.refresh_token.is_empty() {
                // Try to refresh
                match refresh_access_token(&tokens.refresh_token).await {
                    Ok(new_tokens) => {
                        // Save refreshed tokens
                        let _ = save_tokens(&new_tokens);
                        return Ok(new_tokens);
                    }
                    Err(_) => {
                        // Fall through to new device flow
                    }
                }
            }
        } else {
            return Ok(tokens);
        }
    }

    // No valid tokens - start new device flow
    let tokens = complete_device_flow().await?;
    
    // Save the new tokens
    let _ = save_tokens(&tokens);
    
    Ok(tokens)
}

/// Complete device authorization flow with automatic polling
///
/// This is a convenience function that handles the entire flow:
/// 1. Starts the device flow
/// 2. Returns verification info to display
/// 3. Polls for completion
///
/// The caller should display the verification_uri and user_code to the user
/// Note: This is now deprecated in favor of using start_device_flow and poll_for_token separately
/// for UI-based authentication flows.
pub async fn complete_device_flow() -> Result<AuthTokens, AuthError> {
    let (_verification_uri, _user_code, device_response) = start_device_flow().await?;

    let start_time = Instant::now();
    let expires_at = start_time + Duration::from_secs(device_response.expires_in);
    let mut poll_interval = Duration::from_secs(device_response.interval);

    loop {
        if Instant::now() >= expires_at {
            return Err(AuthError::ExpiredToken);
        }

        tokio::time::sleep(poll_interval).await;

        match poll_for_token(&device_response.device_code).await {
            Ok(mut tokens) => {
                tokens.update_expiration();
                return Ok(tokens);
            }
            Err(AuthError::AuthorizationPending) => {
                // Keep waiting
            }
            Err(AuthError::SlowDown) => {
                poll_interval += Duration::from_secs(5);
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}

