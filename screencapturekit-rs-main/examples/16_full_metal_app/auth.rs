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
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
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

    println!("🔐 Requesting device code from Auth0...");

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

    println!("✅ Device code received");
    println!("   User code: {}", device_response.user_code);
    println!("   Verification URI: {}", device_response.verification_uri);
    println!("   Expires in: {} seconds", device_response.expires_in);
    println!("   Poll interval: {} seconds", device_response.interval);

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

    println!("🔄 Refreshing access token...");

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

            println!("✅ Token refreshed successfully!");
            
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
    println!("💾 Tokens saved to: {}", path.display());
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
            Ok(tokens) => {
                println!("📂 Loaded tokens from: {}", path.display());
                Some(tokens)
            }
            Err(e) => {
                eprintln!("⚠️  Failed to parse token file: {}", e);
                None
            }
        },
        Err(e) => {
            eprintln!("⚠️  Failed to read token file: {}", e);
            None
        }
    }
}

/// Logout - delete stored tokens
pub fn logout() -> Result<(), std::io::Error> {
    let path = get_token_file_path();
    if path.exists() {
        fs::remove_file(&path)?;
        println!("🚪 Logged out - tokens deleted from: {}", path.display());
    } else {
        println!("ℹ️  No tokens to delete");
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
    if let Some(mut tokens) = load_tokens() {
        if tokens.is_expired() {
            println!("⚠️  Access token expired");
            
            if !tokens.refresh_token.is_empty() {
                // Try to refresh
                match refresh_access_token(&tokens.refresh_token).await {
                    Ok(new_tokens) => {
                        // Save refreshed tokens
                        if let Err(e) = save_tokens(&new_tokens) {
                            eprintln!("⚠️  Failed to save refreshed tokens: {}", e);
                        }
                        return Ok(new_tokens);
                    }
                    Err(e) => {
                        eprintln!("⚠️  Token refresh failed: {}", e);
                        eprintln!("   Starting new authentication...");
                        // Fall through to new device flow
                    }
                }
            } else {
                println!("⚠️  No refresh token available, need to re-authenticate");
            }
        } else {
            println!("✅ Using cached tokens (valid for {} more seconds)", 
                tokens.expires_at.saturating_sub(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                )
            );
            return Ok(tokens);
        }
    } else {
        println!("ℹ️  No cached tokens found");
    }

    // No valid tokens - start new device flow
    let tokens = complete_device_flow().await?;
    
    // Save the new tokens
    if let Err(e) = save_tokens(&tokens) {
        eprintln!("⚠️  Failed to save tokens: {}", e);
    }
    
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
pub async fn complete_device_flow() -> Result<AuthTokens, AuthError> {
    let (verification_uri, user_code, device_response) = start_device_flow().await?;

    println!("\n╔═══════════════════════════════════════════════════╗");
    println!("║           🔐 AUTHENTICATION REQUIRED              ║");
    println!("╠═══════════════════════════════════════════════════╣");
    println!("║                                                   ║");
    println!("║  1. Open this URL in your browser:               ║");
    println!("║     {}                  ║", verification_uri);
    println!("║                                                   ║");
    println!("║  2. Enter this code:                              ║");
    println!("║     {}                               ║", user_code);
    println!("║                                                   ║");
    println!("║  Waiting for you to complete authentication...   ║");
    println!("╚═══════════════════════════════════════════════════╝\n");

    let start_time = Instant::now();
    let expires_at = start_time + Duration::from_secs(device_response.expires_in);
    let mut poll_interval = Duration::from_secs(device_response.interval);

    loop {
        if Instant::now() >= expires_at {
            return Err(AuthError::ExpiredToken);
        }

        tokio::time::sleep(poll_interval).await;

        print!("⏳ Polling for authorization... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match poll_for_token(&device_response.device_code).await {
            Ok(mut tokens) => {
                println!("✅ Success!\n");
                println!("╔═══════════════════════════════════════════════════╗");
                println!("║         🎉 AUTHENTICATION SUCCESSFUL              ║");
                println!("╚═══════════════════════════════════════════════════╝\n");
                tokens.update_expiration();
                return Ok(tokens);
            }
            Err(AuthError::AuthorizationPending) => {
                println!("⏳ Still waiting...");
            }
            Err(AuthError::SlowDown) => {
                println!("⚠️  Slowing down polling...");
                poll_interval += Duration::from_secs(5);
            }
            Err(e) => {
                println!("❌ Error: {}", e);
                return Err(e);
            }
        }

        let elapsed = Instant::now().duration_since(start_time).as_secs();
        let remaining = device_response.expires_in.saturating_sub(elapsed);
        if remaining < 60 {
            println!("⚠️  Code expires in {} seconds", remaining);
        }
    }
}

