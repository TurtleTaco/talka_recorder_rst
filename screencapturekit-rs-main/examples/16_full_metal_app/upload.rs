//! Upload API for storage.talka.ai
//!
//! Handles uploading recordings to the Talka storage service

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const STORAGE_BASE_URL: &str = "https://storage.talka.ai";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadStatus {
    Idle,
    CreatingFile,
    UploadingFile { percent: u8 },
    CreatingMetadata,
    Complete,
    Failed(String),
}

impl Default for UploadStatus {
    fn default() -> Self {
        Self::Idle
    }
}

impl UploadStatus {
    pub fn as_display_string(&self) -> String {
        match self {
            Self::Idle => "Ready".to_string(),
            Self::CreatingFile => "Creating file entry...".to_string(),
            Self::UploadingFile { percent } => format!("Uploading... {}%", percent),
            Self::CreatingMetadata => "Creating metadata...".to_string(),
            Self::Complete => "Upload complete!".to_string(),
            Self::Failed(err) => format!("Upload failed: {}", err),
        }
    }
}

#[derive(Debug, Serialize)]
struct CreateFileRequest {
    name: String,
    #[serde(rename = "file-type")]
    file_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateFileResponse {
    pub file_id: String,
    pub upload_url: String,
}

#[derive(Debug, Serialize)]
pub struct CallMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webcam_primary_user: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(default)]
    pub speakers: Vec<String>,
    pub file_id: String,
}

#[derive(Debug)]
pub enum UploadError {
    Network(String),
    Io(String),
    InvalidToken,
    InvalidResponse(String),
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {}", msg),
            Self::Io(msg) => write!(f, "I/O error: {}", msg),
            Self::InvalidToken => write!(f, "Invalid or expired access token"),
            Self::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for UploadError {}

/// Infer file type from file extension
fn infer_file_type(file_name: &str) -> String {
    let lower = file_name.to_lowercase();
    
    // Check for audio extensions
    if lower.ends_with(".mp3") || lower.ends_with(".flac") || 
       lower.ends_with(".wav") || lower.ends_with(".m4a") || 
       lower.ends_with(".aac") {
        return "mp3".to_string();
    }
    
    // Check for video extensions
    if lower.ends_with(".mp4") || lower.ends_with(".mov") || 
       lower.ends_with(".m4v") || lower.ends_with(".webm") || 
       lower.ends_with(".avi") {
        return "mp4".to_string();
    }
    
    // Default to mp4
    "mp4".to_string()
}

/// Step 1: Create a file entry in the storage system
pub async fn create_file(
    access_token: &str,
    file_name: &str,
) -> Result<CreateFileResponse, UploadError> {
    println!("[UPLOAD] Creating file entry: {}", file_name);
    
    let file_type = infer_file_type(file_name);
    println!("[UPLOAD] File type: {}", file_type);
    
    let client = reqwest::Client::new();
    let url = format!("{}/files/v2", STORAGE_BASE_URL);
    
    // Create multipart form
    let form = reqwest::multipart::Form::new()
        .text("name", file_name.to_string())
        .text("file-type", file_type);
    
    let response = client
        .post(&url)
        .header("Authorization", access_token)
        .header("Accept", "application/json")
        .multipart(form)
        .send()
        .await
        .map_err(|e| UploadError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(UploadError::Network(format!("HTTP {}: {}", status, text)));
    }
    
    let create_response: CreateFileResponse = response
        .json()
        .await
        .map_err(|e| UploadError::InvalidResponse(e.to_string()))?;
    
    println!("[UPLOAD] File entry created: {}", create_response.file_id);
    Ok(create_response)
}

/// Step 2: Upload the file binary to the presigned URL
pub async fn upload_file(
    upload_url: &str,
    file_path: &Path,
    progress_tracker: Option<Arc<AtomicUsize>>,
) -> Result<(), UploadError> {
    println!("[UPLOAD] Uploading file: {}", file_path.display());
    
    // Read file
    let file_data = tokio::fs::read(file_path)
        .await
        .map_err(|e| UploadError::Io(e.to_string()))?;
    
    let file_size = file_data.len();
    println!("[UPLOAD] File size: {} bytes", file_size);
    
    let client = reqwest::Client::new();
    
    // Upload
    let response = client
        .put(upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", file_size)
        .body(file_data)
        .send()
        .await
        .map_err(|e| UploadError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(UploadError::Network(format!("HTTP {}: {}", status, text)));
    }
    
    // Update progress to 100%
    if let Some(tracker) = progress_tracker {
        tracker.store(100, Ordering::Relaxed);
    }
    
    println!("[UPLOAD] File uploaded successfully");
    Ok(())
}

/// Step 3: Create call metadata associated with the file
pub async fn create_call_metadata(
    access_token: &str,
    file_id: &str,
    metadata: CallMetadata,
) -> Result<(), UploadError> {
    println!("[UPLOAD] Creating call metadata for file: {}", file_id);
    
    let client = reqwest::Client::new();
    let url = format!("{}/files/v2/{}/call", STORAGE_BASE_URL, file_id);
    
    let response = client
        .post(&url)
        .header("Authorization", access_token)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&metadata)
        .send()
        .await
        .map_err(|e| UploadError::Network(e.to_string()))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(UploadError::Network(format!("HTTP {}: {}", status, text)));
    }
    
    println!("[UPLOAD] Call metadata created successfully");
    Ok(())
}

/// Complete upload workflow: create file, upload, and create metadata
pub async fn upload_recording(
    access_token: &str,
    file_path: &Path,
    title: Option<String>,
    status_callback: Option<Box<dyn Fn(UploadStatus) + Send + Sync>>,
) -> Result<String, UploadError> {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| UploadError::Io("Invalid file name".to_string()))?;
    
    // Step 1: Create file entry
    if let Some(ref cb) = status_callback {
        cb(UploadStatus::CreatingFile);
    }
    let create_response = create_file(access_token, file_name).await?;
    
    // Step 2: Upload file
    if let Some(ref cb) = status_callback {
        cb(UploadStatus::UploadingFile { percent: 0 });
    }
    let progress_tracker = Arc::new(AtomicUsize::new(0));
    let progress_clone = Arc::clone(&progress_tracker);
    
    // Spawn progress updater
    if let Some(ref cb) = status_callback {
        let cb_clone = cb.clone();
        tokio::spawn(async move {
            loop {
                let percent = progress_clone.load(Ordering::Relaxed);
                if percent >= 100 {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    }
    
    upload_file(&create_response.upload_url, file_path, Some(progress_tracker)).await?;
    
    if let Some(ref cb) = status_callback {
        cb(UploadStatus::UploadingFile { percent: 100 });
    }
    
    // Step 3: Create call metadata
    if let Some(ref cb) = status_callback {
        cb(UploadStatus::CreatingMetadata);
    }
    
    let metadata = CallMetadata {
        title,
        recorded_datetime: Some(chrono::Utc::now().to_rfc3339()),
        provider: Some("Talka Cap Pro".to_string()),
        webcam_primary_user: None,
        is_private: Some(false),
        speakers: vec![],
        file_id: create_response.file_id.clone(),
    };
    
    create_call_metadata(access_token, &create_response.file_id, metadata).await?;
    
    if let Some(ref cb) = status_callback {
        cb(UploadStatus::Complete);
    }
    
    Ok(create_response.file_id)
}

