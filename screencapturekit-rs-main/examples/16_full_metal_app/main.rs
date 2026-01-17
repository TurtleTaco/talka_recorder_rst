//! Full Metal Application with Modern Dioxus UI
//!
//! A complete macOS application demonstrating the full ScreenCaptureKit API with a modern Dioxus UI:
//!
//! - **Modern Web UI** - Clean, responsive interface built with Dioxus
//! - **Metal GPU Rendering** - Hardware-accelerated graphics (background)
//! - **Screen Capture** - Real-time display/window capture via ScreenCaptureKit
//! - **Content Picker** - System UI for selecting capture source (macOS 14.0+)
//! - **Video Recording** - Direct-to-file recording (macOS 15.0+)
//! - **Microphone Capture** - Audio input with device selection (macOS 15.0+)
//!
//! ## Running
//!
//! ```bash
//! # Basic (macOS 14.0+)
//! cargo run --example 16_full_metal_app --features macos_14_0
//!
//! # With recording support (macOS 15.0+)
//! cargo run --example 16_full_metal_app --features macos_15_0
//! ```

#![allow(
    clippy::too_many_lines,
    clippy::useless_transmute,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

mod auth;
mod capture;
mod font;
mod input;
mod overlay;
mod preview_window;
#[cfg(feature = "macos_15_0")]
mod recording;
mod renderer;
mod screenshot;
mod ui;
#[cfg(feature = "macos_15_0")]
mod upload;
mod vertex;
mod waveform;
mod dioxus_ui;

use dioxus::prelude::*;
use dioxus::desktop::{Config, WindowBuilder};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use tokio::runtime::Runtime;

use screencapturekit::prelude::*;

use capture::CaptureState;
use input::{format_picked_source, PickerResult};
use overlay::default_stream_config;
use dioxus_ui::CaptureCommand;

#[cfg(feature = "macos_15_0")]
use recording::{RecordingConfig, RecordingState};

#[derive(Clone, Debug, PartialEq)]
enum AuthState {
    Checking,
    NeedsAuth { verification_uri: String, user_code: String },
    Authenticating,
    Authenticated { profile: auth::UserProfile },
    Error(String),
}

fn main() {
    run_app();
}

fn run_app() {
    // Create runtime for async operations
    let runtime = Runtime::new().unwrap();
    let runtime_handle = runtime.handle().clone();

    // Create communication channel between UI and capture backend
    let (cmd_tx, cmd_rx): (Sender<CaptureCommand>, Receiver<CaptureCommand>) = channel();
    
    // Shared state
    let is_capturing = Arc::new(AtomicBool::new(false));
    let is_recording = Arc::new(AtomicBool::new(false));
    let source_name = Arc::new(Mutex::new(String::from("No source selected")));
    let auth_state_shared: Arc<Mutex<AuthState>> = Arc::new(Mutex::new(AuthState::Checking));
    let upload_status_str = Arc::new(Mutex::new(String::from("")));
    let uploaded_file_id = Arc::new(Mutex::new(String::new()));
    
    // Shared auth tokens for upload
    let auth_tokens_shared: Arc<Mutex<Option<auth::AuthTokens>>> = Arc::new(Mutex::new(None));
    
    // Shared meeting events
    let meeting_events_shared: Arc<Mutex<Vec<auth::MeetingEvent>>> = Arc::new(Mutex::new(Vec::new()));

    // Start authentication in background
    let auth_state_clone = Arc::clone(&auth_state_shared);
    let auth_tokens_clone = Arc::clone(&auth_tokens_shared);
    runtime_handle.spawn(async move {
        match authenticate_user_with_ui(&auth_state_clone).await {
            Ok((tokens, profile)) => {
                *auth_state_clone.lock().unwrap() = AuthState::Authenticated { profile };
                *auth_tokens_clone.lock().unwrap() = Some(tokens);
                println!("✅ Authenticated successfully");
            }
            Err(e) => {
                *auth_state_clone.lock().unwrap() = AuthState::Error(e.clone());
                eprintln!("❌ Authentication failed: {}", e);
            }
        }
    });

    // Create capture state
    let capture_state = Arc::new(CaptureState::new());
    
    // Start capture backend thread
    let is_capturing_clone = Arc::clone(&is_capturing);
    let is_recording_clone = Arc::clone(&is_recording);
    let source_name_clone = Arc::clone(&source_name);
    let upload_status_clone = Arc::clone(&upload_status_str);
    let uploaded_file_id_clone = Arc::clone(&uploaded_file_id);
    let capture_state_backend = Arc::clone(&capture_state);
    let auth_tokens_backend = Arc::clone(&auth_tokens_shared);
    let runtime_handle_capture = runtime_handle.clone();
    
    thread::spawn(move || {
        run_capture_backend(
            cmd_rx,
            is_capturing_clone,
            is_recording_clone,
            source_name_clone,
            upload_status_clone,
            uploaded_file_id_clone,
            runtime_handle_capture,
            capture_state_backend,
            auth_tokens_backend,
        );
    });

    // Store state in static globals for the Dioxus app
    unsafe {
        GLOBAL_CMD_TX = Some(cmd_tx);
        GLOBAL_IS_CAPTURING = Some(is_capturing);
        GLOBAL_IS_RECORDING = Some(is_recording);
        GLOBAL_SOURCE_NAME = Some(source_name);
        GLOBAL_AUTH_STATE = Some(auth_state_shared.clone());
        GLOBAL_UPLOAD_STATUS = Some(upload_status_str);
        GLOBAL_UPLOADED_FILE_ID = Some(uploaded_file_id);
        GLOBAL_CAPTURE_STATE = Some(capture_state);
        GLOBAL_MEETING_EVENTS = Some(meeting_events_shared.clone());
        GLOBAL_AUTH_TOKENS = Some(auth_tokens_shared.clone());
    }
    
    // Start meeting events fetching in background
    let meeting_events_clone = Arc::clone(&meeting_events_shared);
    let auth_tokens_fetch = Arc::clone(&auth_tokens_shared);
    let runtime_handle_events = runtime_handle.clone();
    
    // Initial fetch on startup (try immediately after auth)
    let meeting_events_initial = Arc::clone(&meeting_events_shared);
    let auth_tokens_initial = Arc::clone(&auth_tokens_shared);
    runtime_handle.spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        let access_token = {
            let guard = auth_tokens_initial.lock().unwrap();
            guard.as_ref().map(|t| t.access_token.clone())
        };
        
        if let Some(token) = access_token {
            match auth::get_meeting_events(&token).await {
                Ok(events) => {
                    *meeting_events_initial.lock().unwrap() = events;
                }
                Err(_) => {}
            }
        }
    });
    
    // Periodic refresh every 5 minutes
    runtime_handle_events.spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
            
            let access_token = {
                let guard = auth_tokens_fetch.lock().unwrap();
                guard.as_ref().map(|t| t.access_token.clone())
            };
            
            if let Some(token) = access_token {
                match auth::get_meeting_events(&token).await {
                    Ok(events) => {
                        *meeting_events_clone.lock().unwrap() = events;
                    }
                    Err(_) => {}
                }
            }
        }
    });

    // Launch Dioxus UI with custom window config
    let config = Config::new()
        .with_window(WindowBuilder::new()
            .with_title("Talka Recorder")
            .with_resizable(false)
            .with_inner_size(dioxus::desktop::wry::dpi::LogicalSize::new(320.0, 440.0)));
    
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(app_with_backend);
}

// Global state for Dioxus app
static mut GLOBAL_CMD_TX: Option<Sender<CaptureCommand>> = None;
static mut GLOBAL_IS_CAPTURING: Option<Arc<AtomicBool>> = None;
static mut GLOBAL_IS_RECORDING: Option<Arc<AtomicBool>> = None;
static mut GLOBAL_SOURCE_NAME: Option<Arc<Mutex<String>>> = None;
static mut GLOBAL_AUTH_STATE: Option<Arc<Mutex<AuthState>>> = None;
static mut GLOBAL_UPLOAD_STATUS: Option<Arc<Mutex<String>>> = None;
static mut GLOBAL_UPLOADED_FILE_ID: Option<Arc<Mutex<String>>> = None;
static mut GLOBAL_CAPTURE_STATE: Option<Arc<CaptureState>> = None;
static mut GLOBAL_MEETING_EVENTS: Option<Arc<Mutex<Vec<auth::MeetingEvent>>>> = None;
static mut GLOBAL_AUTH_TOKENS: Option<Arc<Mutex<Option<auth::AuthTokens>>>> = None;

fn get_global_state() -> (
    Option<Sender<CaptureCommand>>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Mutex<String>>,
    Arc<Mutex<AuthState>>,
    Arc<Mutex<String>>,
    Arc<Mutex<String>>,
    Arc<CaptureState>,
    Arc<Mutex<Vec<auth::MeetingEvent>>>,
    Arc<Mutex<Option<auth::AuthTokens>>>,
) {
    unsafe {
        (
            GLOBAL_CMD_TX.clone(),
            GLOBAL_IS_CAPTURING.clone().unwrap(),
            GLOBAL_IS_RECORDING.clone().unwrap(),
            GLOBAL_SOURCE_NAME.clone().unwrap(),
            GLOBAL_AUTH_STATE.clone().unwrap(),
            GLOBAL_UPLOAD_STATUS.clone().unwrap(),
            GLOBAL_UPLOADED_FILE_ID.clone().unwrap(),
            GLOBAL_CAPTURE_STATE.clone().unwrap(),
            GLOBAL_MEETING_EVENTS.clone().unwrap(),
            GLOBAL_AUTH_TOKENS.clone().unwrap(),
        )
    }
}

fn app_with_backend() -> Element {
    let (_cmd_tx, is_capturing, is_recording, source_name, auth_state, upload_status, uploaded_file_id, _capture_state, meeting_events, auth_tokens) = get_global_state();

    let mut is_capturing_sig = use_signal(|| is_capturing.load(Ordering::Relaxed));
    let mut is_recording_sig = use_signal(|| is_recording.load(Ordering::Relaxed));
    let mut source_name_sig = use_signal(|| source_name.lock().unwrap().clone());
    let mut auth_state_sig = use_signal(|| auth_state.lock().unwrap().clone());
    let mut upload_status_sig = use_signal(|| upload_status.lock().unwrap().clone());
    let mut uploaded_file_id_sig = use_signal(|| uploaded_file_id.lock().unwrap().clone());
    let mut frame_count_sig = use_signal(|| 0u64);
    let mut capture_info_sig = use_signal(|| String::from(""));
    let mut recording_duration_sig = use_signal(|| String::from(""));
    let mut recording_start_time_sig = use_signal(|| None::<std::time::Instant>);
    let mut meeting_events_sig = use_signal(|| meeting_events.lock().unwrap().clone());
    let mut show_calendar_view = use_signal(|| false);

    // Poll for updates every 100ms
    use_future(move || async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let (_, is_cap, is_rec, src_name, auth, upl, file_id, cap_state, mtg_events, _) = get_global_state();
            let was_recording = *is_recording_sig.read();
            let is_recording_now = is_rec.load(Ordering::Relaxed);
            
            is_capturing_sig.set(is_cap.load(Ordering::Relaxed));
            is_recording_sig.set(is_recording_now);
            source_name_sig.set(src_name.lock().unwrap().clone());
            auth_state_sig.set(auth.lock().unwrap().clone());
            upload_status_sig.set(upl.lock().unwrap().clone());
            uploaded_file_id_sig.set(file_id.lock().unwrap().clone());
            meeting_events_sig.set(mtg_events.lock().unwrap().clone());
            
            // Track recording start time
            if is_recording_now && !was_recording {
                recording_start_time_sig.set(Some(std::time::Instant::now()));
            } else if !is_recording_now && was_recording {
                recording_start_time_sig.set(None);
                recording_duration_sig.set(String::new());
            }
            
            // Update recording duration
            if is_recording_now {
                if let Some(start_time) = *recording_start_time_sig.read() {
                    let elapsed = start_time.elapsed();
                    let secs = elapsed.as_secs();
                    let mins = secs / 60;
                    let secs = secs % 60;
                    recording_duration_sig.set(format!("{:02}:{:02}", mins, secs));
                }
            }
            
            // Update frame count and capture info
            let frame_count = cap_state.frame_count.load(Ordering::Relaxed);
            frame_count_sig.set(frame_count as u64);
            
            if is_cap.load(Ordering::Relaxed) {
                // Get surface dimensions
                let surface_info = if let Ok(guard) = cap_state.latest_surface.try_lock() {
                    if let Some(ref surface) = *guard {
                        format!("{}x{}", surface.width(), surface.height())
                    } else {
                        "Waiting...".to_string()
                    }
                } else {
                    "Processing...".to_string()
                };
                capture_info_sig.set(format!("{} frames | {}", frame_count, surface_info));
            } else {
                capture_info_sig.set(String::new());
            }
        }
    });

    rsx! {
        style { {include_str!("./assets/main.css")} }
        
        // Show login overlay if not authenticated
        if !matches!(*auth_state_sig.read(), AuthState::Authenticated { .. }) {
            LoginOverlay { auth_state: auth_state_sig.read().clone() }
        } else {
            div { id: "app",
                // Header with logo and profile
                Header { 
                    auth_state: auth_state_sig.read().clone(),
                    show_calendar_view: *show_calendar_view.read(),
                    on_calendar_click: move |_| {
                        let current = *show_calendar_view.read();
                        show_calendar_view.set(!current);
                        
                        // Refresh meeting events when calendar is opened
                        if !current {
                            let (_, _, _, _, _, _, _, _, mtg_events, auth_tkns) = get_global_state();
                            
                            let events_clone = Arc::clone(&mtg_events);
                            let tokens_clone = Arc::clone(&auth_tkns);
                            
                            tokio::spawn(async move {
                                let access_token = {
                                    let guard = tokens_clone.lock().unwrap();
                                    guard.as_ref().map(|t| t.access_token.clone())
                                };
                                
                                if let Some(token) = access_token {
                                    match auth::get_meeting_events(&token).await {
                                        Ok(events) => {
                                            *events_clone.lock().unwrap() = events;
                                        }
                                        Err(_) => {}
                                    }
                                }
                            });
                        }
                    },
                }
                
                // Calendar events view (full overlay)
                if *show_calendar_view.read() {
                    CalendarEventsView {
                        events: meeting_events_sig.read().clone(),
                        on_close: move |_| {
                            show_calendar_view.set(false);
                        },
                    }
                } else {
                    // Next meeting notification bar (when calendar is closed)
                    NextMeetingNotification {
                        events: meeting_events_sig.read().clone(),
                    }
                    
                    // Main content area - centered
                    MainContent { 
                        is_capturing: *is_capturing_sig.read(),
                        is_recording: *is_recording_sig.read(),
                        source_name: source_name_sig.read().clone(),
                        recording_duration: recording_duration_sig.read().clone(),
                        upload_status: upload_status_sig.read().clone(),
                        uploaded_file_id: uploaded_file_id_sig.read().clone(),
                    }
                }
            }
        }
    }
}

// Login overlay component
#[component]
fn LoginOverlay(auth_state: AuthState) -> Element {
    const LOGO_SVG: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMzkxIiBoZWlnaHQ9IjE2OCIgdmlld0JveD0iMCAwIDM5MSAxNjgiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CjxyZWN0IHg9IjI0IiB5PSI1MiIgd2lkdGg9IjI0IiBoZWlnaHQ9IjY0IiByeD0iMTIiIGZpbGw9IiM2NDhGRkYiLz4KPHJlY3QgeD0iNTYiIHk9IjM0IiB3aWR0aD0iMjQiIGhlaWdodD0iMTAwIiByeD0iMTIiIGZpbGw9IiMyNkM0ODUiLz4KPHJlY3QgeD0iODgiIHk9IjUyIiB3aWR0aD0iMjQiIGhlaWdodD0iNjQiIHJ4PSIxMiIgZmlsbD0iI0UwMUU1QSIvPgo8cmVjdCB4PSIxMjAiIHk9IjY4IiB3aWR0aD0iMjQiIGhlaWdodD0iMzIiIHJ4PSIxMiIgZmlsbD0iI0Y2QUUyRCIvPgo8cGF0aCBkPSJNMjA3LjA0IDc0LjE2VjEwMEMyMDcuMDQgMTAyLjEzMyAyMDcuNDkzIDEwMy42NTMgMjA4LjQgMTA0LjU2QzIwOS4zMDcgMTA1LjQxMyAyMTAuODggMTA1Ljg0IDIxMy4xMiAxMDUuODRIMjE4LjQ4VjExMkgyMTEuOTJDMjA3Ljg2NyAxMTIgMjA0LjgyNyAxMTEuMDY3IDIwMi44IDEwOS4yQzIwMC43NzMgMTA3LjMzMyAxOTkuNzYgMTA0LjI2NyAxOTkuNzYgMTAwVjc0LjE2SDE5NC4wOFY2OC4xNkgxOTkuNzZWNTcuMTJIMjA3LjA0VjY4LjE2SDIxOC40OFY3NC4xNkgyMDcuMDRaIiBmaWxsPSJibGFjayIvPgo8cGF0aCBkPSJNMjI0LjU4MSA4OS45MkMyMjQuNTgxIDg1LjQ0IDIyNS40ODcgODEuNTIgMjI3LjMwMSA3OC4xNkMyMjkuMTE0IDc0Ljc0NjcgMjMxLjU5NCA3Mi4xMDY3IDIzNC43NDEgNzAuMjRDMjM3Ljk0MSA2OC4zNzMzIDI0MS40ODcgNjcuNDQgMjQ1LjM4MSA2Ny40NEMyNDkuMjIxIDY3LjQ0IDI1Mi41NTQgNjguMjY2NyAyNTUuMzgxIDY5LjkyQzI1OC4yMDcgNzEuNTczMyAyNjAuMzE0IDczLjY1MzMgMjYxLjcwMSA3Ni4xNlY2OC4xNkgyNjkuMDYxVjExMkgyNjEuNzAxVjEwMy44NEMyNjAuMjYxIDEwNi40IDI1OC4xMDEgMTA4LjUzMyAyNTUuMjIxIDExMC4yNEMyNTIuMzk0IDExMS44OTMgMjQ5LjA4NyAxMTIuNzIgMjQ1LjMwMSAxMTIuNzJDMjQxLjQwNyAxMTIuNzIgMjM3Ljg4NyAxMTEuNzYgMjM0Ljc0MSAxMDkuODRDMjMxLjU5NCAxMDcuOTIgMjI5LjExNCAxMDUuMjI3IDIyNy4zMDEgMTAxLjc2QzIyNS40ODcgOTguMjkzMyAyMjQuNTgxIDk0LjM0NjcgMjI0LjU4MSA4OS45MlpNMjYxLjcwMSA5MEMyNjEuNzAxIDg2LjY5MzMgMjYxLjAzNCA4My44MTMzIDI1OS43MDEgODEuMzZDMjU4LjM2NyA3OC45MDY3IDI1Ni41NTQgNzcuMDQgMjU0LjI2MSA3NS43NkMyNTIuMDIxIDc0LjQyNjcgMjQ5LjU0MSA3My43NiAyNDYuODIxIDczLjc2QzI0NC4xMDEgNzMuNzYgMjQxLjYyMSA3NC40IDIzOS4zODEgNzUuNjhDMjM3LjE0MSA3Ni45NiAyMzUuMzU0IDc4LjgyNjcgMjM0LjAyMSA4MS4yOEMyMzIuNjg3IDgzLjczMzMgMjMyLjAyMSA4Ni42MTMzIDIzMi4wMjEgODkuOTJDMjMyLjAyMSA5My4yOCAyMzIuNjg3IDk2LjIxMzMgMjM0LjAyMSA5OC43MkMyMzUuMzU0IDEwMS4xNzMgMjM3LjE0MSAxMDMuMDY3IDIzOS4zODEgMTA0LjRDMjQxLjYyMSAxMDUuNjggMjQ0LjEwMSAxMDYuMzIgMjQ2LjgyMSAxMDYuMzJDMjQ5LjU0MSAxMDYuMzIgMjUyLjAyMSAxMDUuNjggMjU0LjI2MSAxMDQuNEMyNTYuNTU0IDEwMy4wNjcgMjU4LjM2NyAxMDEuMTczIDI1OS43MDEgOTguNzJDMjYxLjAzNCA5Ni4yMTMzIDI2MS43MDEgOTMuMzA2NyAyNjEuNzAxIDkwWiIgZmlsbD0iYmxhY2siLz4KPHBhdGggZD0iTTI4OC42NDMgNTIuOFYxMTJIMjgxLjM2M1Y1Mi44SDI4OC42NDNaIiBmaWxsPSJibGFjayIvPgo8cGF0aCBkPSJNMzI1LjUzMSAxMTJMMzA4LjMzMSA5Mi42NFYxMTJIMzAxLjA1MVY1Mi44SDMwOC4zMzFWODcuNkwzMjUuMjExIDY4LjE2SDMzNS4zNzFMMzE0LjczMSA5MEwzMzUuNDUxIDExMkgzMjUuNTMxWiIgZmlsbD0iYmxhY2siLz4KPHBhdGggZD0iTTMzOS41MDMgODkuOTJDMzM5LjUwMyA4NS40NCAzNDAuNDA5IDgxLjUyIDM0Mi4yMjMgNzguMTZDMzQ0LjAzNiA3NC43NDY3IDM0Ni41MTYgNzIuMTA2NyAzNDkuNjYzIDcwLjI0QzM1Mi44NjMgNjguMzczMyAzNTYuNDA5IDY3LjQ0IDM2MC4zMDMgNjcuNDRDMzY0LjE0MyA2Ny40NCAzNjcuNDc2IDY4LjI2NjcgMzcwLjMwMyA2OS45MkMzNzMuMTI5IDcxLjU3MzMgMzc1LjIzNiA3My42NTMzIDM3Ni42MjMgNzYuMTZWNjguMTZIMzgzLjk4M1YxMTJIMzc2LjYyM1YxMDMuODRDMzc1LjE4MyAxMDYuNCAzNzMuMDIzIDEwOC41MzMgMzcwLjE0MyAxMTAuMjRDMzY3LjMxNiAxMTEuODkzIDM2NC4wMDkgMTEyLjcyIDM2MC4yMjMgMTEyLjcyQzM1Ni4zMjkgMTEyLjcyIDM1Mi44MDkgMTExLjc2IDM0OS42NjMgMTA5Ljg0QzM0Ni41MTYgMTA3LjkyIDM0NC4wMzYgMTA1LjIyNyAzNDIuMjIzIDEwMS43NkMzNDAuNDA5IDk4LjI5MzMgMzM5LjUwMyA5NC4zNDY3IDMzOS41MDMgODkuOTJaTTM3Ni42MjMgOTBDMzc2LjYyMyA4Ni42OTMzIDM3NS45NTYgODMuODEzMyAzNzQuNjIzIDgxLjM2QzM3My4yODkgNzguOTA2NyAzNzEuNDc2IDc3LjA0IDM2OS4xODMgNzUuNzZDMzY2Ljk0MyA3NC40MjY3IDM2NC40NjMgNzMuNzYgMzYxLjc0MyA3My43NkMzNTkuMDIzIDczLjc2IDM1Ni41NDMgNzQuNCAzNTQuMzAzIDc1LjY4QzM1Mi4wNjMgNzYuOTYgMzUwLjI3NiA3OC44MjY3IDM0OC45NDMgODEuMjhDMzQ3LjYwOSA4My43MzMzIDM0Ni45NDMgODYuNjEzMyAzNDYuOTQzIDg5LjkyQzM0Ni45NDMgOTMuMjggMzQ3LjYwOSA5Ni4yMTMzIDM0OC45NDMgOTguNzJDMzUwLjI3NiAxMDEuMTczIDM1Mi4wNjMgMTAzLjA2NyAzNTQuMzAzIDEwNC40QzM1Ni41NDMgMTA1LjY4IDM1OS4wMjMgMTA2LjMyIDM2MS43NDMgMTA2LjMyQzM2NC40NjMgMTA2LjMyIDM2Ni45NDMgMTA1LjY4IDM2OS4xODMgMTA0LjRDMzcxLjQ3NiAxMDMuMDY3IDM3My4yODkgMTAxLjE3MyAzNzQuNjIzIDk4LjcyQzM3NS45NTYgOTYuMjEzMyAzNzYuNjIzIDkzLjMwNjcgMzc2LjYyMyA5MFoiIGZpbGw9ImJsYWNrIi8+Cjwvc3ZnPgo=";
    
    rsx! {
        div { class: "login-overlay",
            div { class: "login-container",
                img { 
                    class: "login-logo",
                    src: "{LOGO_SVG}",
                }
                
                match auth_state {
                    AuthState::Checking => rsx! {
                        h2 { class: "login-title", "Welcome to Talka Recorder" }
                        p { class: "login-subtitle", "Checking authentication..." }
                        div { class: "waiting-message",
                            div { class: "spinner" }
                            span { "Loading..." }
                        }
                    },
                    AuthState::NeedsAuth { ref verification_uri, ref user_code } => {
                        let uri_clone = verification_uri.clone();
                        let code_clone = user_code.clone();
                        rsx! {
                            h2 { class: "login-title", "Sign in to Continue" }
                            p { class: "login-subtitle", "Complete authentication in your browser" }
                            
                            div { class: "login-step",
                                p { class: "step-label", "Step 1: Open this URL in your browser" }
                                div { class: "code-box",
                                    span { class: "url-text", "{verification_uri}" }
                                    button { 
                                        class: "copy-btn",
                                        onclick: move |_| {
                                            // Copy to clipboard
                                            let _ = copy_to_clipboard(&uri_clone);
                                        },
                                        "Copy"
                                    }
                                }
                            }
                            
                            div { class: "login-step",
                                p { class: "step-label", "Step 2: Enter this code" }
                                div { class: "code-box",
                                    span { class: "code-text", "{user_code}" }
                                    button { 
                                        class: "copy-btn",
                                        onclick: move |_| {
                                            // Copy to clipboard
                                            let _ = copy_to_clipboard(&code_clone);
                                        },
                                        "Copy"
                                    }
                                }
                            }
                            
                            div { class: "waiting-message",
                                div { class: "spinner" }
                                span { "Waiting for you to complete authentication..." }
                            }
                        }
                    },
                    AuthState::Authenticating => rsx! {
                        h2 { class: "login-title", "Completing Authentication" }
                        p { class: "login-subtitle", "Please wait..." }
                        div { class: "waiting-message",
                            div { class: "spinner" }
                            span { "Finalizing..." }
                        }
                    },
                    AuthState::Error(ref err) => rsx! {
                        h2 { class: "login-title", "Authentication Error" }
                        p { class: "login-subtitle", "{err}" }
                        p { style: "color: #D93025; margin-top: 1rem;", "Please restart the application to try again." }
                    },
                    _ => rsx! { div {} }
                }
            }
        }
    }
}

#[component]
fn Header(auth_state: AuthState, show_calendar_view: bool, on_calendar_click: EventHandler<()>) -> Element {
    const LOGO_SVG: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMzkxIiBoZWlnaHQ9IjE2OCIgdmlld0JveD0iMCAwIDM5MSAxNjgiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CjxyZWN0IHg9IjI0IiB5PSI1MiIgd2lkdGg9IjI0IiBoZWlnaHQ9IjY0IiByeD0iMTIiIGZpbGw9IiM2NDhGRkYiLz4KPHJlY3QgeD0iNTYiIHk9IjM0IiB3aWR0aD0iMjQiIGhlaWdodD0iMTAwIiByeD0iMTIiIGZpbGw9IiMyNkM0ODUiLz4KPHJlY3QgeD0iODgiIHk9IjUyIiB3aWR0aD0iMjQiIGhlaWdodD0iNjQiIHJ4PSIxMiIgZmlsbD0iI0UwMUU1QSIvPgo8cmVjdCB4PSIxMjAiIHk9IjY4IiB3aWR0aD0iMjQiIGhlaWdodD0iMzIiIHJ4PSIxMiIgZmlsbD0iI0Y2QUUyRCIvPgo8cGF0aCBkPSJNMjA3LjA0IDc0LjE2VjEwMEMyMDcuMDQgMTAyLjEzMyAyMDcuNDkzIDEwMy42NTMgMjA4LjQgMTA0LjU2QzIwOS4zMDcgMTA1LjQxMyAyMTAuODggMTA1Ljg0IDIxMy4xMiAxMDUuODRIMjE4LjQ4VjExMkgyMTEuOTJDMjA3Ljg2NyAxMTIgMjA0LjgyNyAxMTEuMDY3IDIwMi44IDEwOS4yQzIwMC43NzMgMTA3LjMzMyAxOTkuNzYgMTA0LjI2NyAxOTkuNzYgMTAwVjc0LjE2SDE5NC4wOFY2OC4xNkgxOTkuNzZWNTcuMTJIMjA3LjA0VjY4LjE2SDIxOC40OFY3NC4xNkgyMDcuMDRaIiBmaWxsPSJibGFjayIvPgo8cGF0aCBkPSJNMjI0LjU4MSA4OS45MkMyMjQuNTgxIDg1LjQ0IDIyNS40ODcgODEuNTIgMjI3LjMwMSA3OC4xNkMyMjkuMTE0IDc0Ljc0NjcgMjMxLjU5NCA3Mi4xMDY3IDIzNC43NDEgNzAuMjRDMjM3Ljk0MSA2OC4zNzMzIDI0MS40ODcgNjcuNDQgMjQ1LjM4MSA2Ny40NEMyNDkuMjIxIDY3LjQ0IDI1Mi41NTQgNjguMjY2NyAyNTUuMzgxIDY5LjkyQzI1OC4yMDcgNzEuNTczMyAyNjAuMzE0IDczLjY1MzMgMjYxLjcwMSA3Ni4xNlY2OC4xNkgyNjkuMDYxVjExMkgyNjEuNzAxVjEwMy44NEMyNjAuMjYxIDEwNi40IDI1OC4xMDEgMTA4LjUzMyAyNTUuMjIxIDExMC4yNEMyNTIuMzk0IDExMS44OTMgMjQ5LjA4NyAxMTIuNzIgMjQ1LjMwMSAxMTIuNzJDMjQxLjQwNyAxMTIuNzIgMjM3Ljg4NyAxMTEuNzYgMjM0Ljc0MSAxMDkuODRDMjMxLjU5NCAxMDcuOTIgMjI5LjExNCAxMDUuMjI3IDIyNy4zMDEgMTAxLjc2QzIyNS40ODcgOTguMjkzMyAyMjQuNTgxIDk0LjM0NjcgMjI0LjU4MSA4OS45MlpNMjYxLjcwMSA5MEMyNjEuNzAxIDg2LjY5MzMgMjYxLjAzNCA4My44MTMzIDI1OS43MDEgODEuMzZDMjU4LjM2NyA3OC45MDY3IDI1Ni41NTQgNzcuMDQgMjU0LjI2MSA3NS43NkMyNTIuMDIxIDc0LjQyNjcgMjQ5LjU0MSA3My43NiAyNDYuODIxIDczLjc2QzI0NC4xMDEgNzMuNzYgMjQxLjYyMSA3NC40IDIzOS4zODEgNzUuNjhDMjM3LjE0MSA3Ni45NiAyMzUuMzU0IDc4LjgyNjcgMjM0LjAyMSA4MS4yOEMyMzIuNjg3IDgzLjczMzMgMjMyLjAyMSA4Ni42MTMzIDIzMi4wMjEgODkuOTJDMjMyLjAyMSA5My4yOCAyMzIuNjg3IDk2LjIxMzMgMjM0LjAyMSA5OC43MkMyMzUuMzU0IDEwMS4xNzMgMjM3LjE0MSAxMDMuMDY3IDIzOS4zODEgMTA0LjRDMjQxLjYyMSAxMDUuNjggMjQ0LjEwMSAxMDYuMzIgMjQ2LjgyMSAxMDYuMzJDMjQ5LjU0MSAxMDYuMzIgMjUyLjAyMSAxMDUuNjggMjU0LjI2MSAxMDQuNEMyNTYuNTU0IDEwMy4wNjcgMjU4LjM2NyAxMDEuMTczIDI1OS43MDEgOTguNzJDMjYxLjAzNCA5Ni4yMTMzIDI2MS43MDEgOTMuMzA2NyAyNjEuNzAxIDkwWiIgZmlsbD0iYmxhY2siLz4KPHBhdGggZD0iTTI4OC42NDMgNTIuOFYxMTJIMjgxLjM2M1Y1Mi44SDI4OC42NDNaIiBmaWxsPSJibGFjayIvPgo8cGF0aCBkPSJNMzI1LjUzMSAxMTJMMzA4LjMzMSA5Mi42NFYxMTJIMzAxLjA1MVY1Mi44SDMwOC4zMzFWODcuNkwzMjUuMjExIDY4LjE2SDMzNS4zNzFMMzE0LjczMSA5MEwzMzUuNDUxIDExMkgzMjUuNTMxWiIgZmlsbD0iYmxhY2siLz4KPHBhdGggZD0iTTMzOS41MDMgODkuOTJDMzM5LjUwMyA4NS40NCAzNDAuNDA5IDgxLjUyIDM0Mi4yMjMgNzguMTZDMzQ0LjAzNiA3NC43NDY3IDM0Ni41MTYgNzIuMTA2NyAzNDkuNjYzIDcwLjI0QzM1Mi44NjMgNjguMzczMyAzNTYuNDA5IDY3LjQ0IDM2MC4zMDMgNjcuNDRDMzY0LjE0MyA2Ny40NCAzNjcuNDc2IDY4LjI2NjcgMzcwLjMwMyA2OS45MkMzNzMuMTI5IDcxLjU3MzMgMzc1LjIzNiA3My42NTMzIDM3Ni42MjMgNzYuMTZWNjguMTZIMzgzLjk4M1YxMTJIMzc2LjYyM1YxMDMuODRDMzc1LjE4MyAxMDYuNCAzNzMuMDIzIDEwOC41MzMgMzcwLjE0MyAxMTAuMjRDMzY3LjMxNiAxMTEuODkzIDM2NC4wMDkgMTEyLjcyIDM2MC4yMjMgMTEyLjcyQzM1Ni4zMjkgMTEyLjcyIDM1Mi44MDkgMTExLjc2IDM0OS42NjMgMTA5Ljg0QzM0Ni41MTYgMTA3LjkyIDM0NC4wMzYgMTA1LjIyNyAzNDIuMjIzIDEwMS43NkMzNDAuNDA5IDk4LjI5MzMgMzM5LjUwMyA5NC4zNDY3IDMzOS41MDMgODkuOTJaTTM3Ni42MjMgOTBDMzc2LjYyMyA4Ni42OTMzIDM3NS45NTYgODMuODEzMyAzNzQuNjIzIDgxLjM2QzM3My4yODkgNzguOTA2NyAzNzEuNDc2IDc3LjA0IDM2OS4xODMgNzUuNzZDMzY2Ljk0MyA3NC40MjY3IDM2NC40NjMgNzMuNzYgMzYxLjc0MyA3My43NkMzNTkuMDIzIDczLjc2IDM1Ni41NDMgNzQuNCAzNTQuMzAzIDc1LjY4QzM1Mi4wNjMgNzYuOTYgMzUwLjI3NiA3OC44MjY3IDM0OC45NDMgODEuMjhDMzQ3LjYwOSA4My43MzMzIDM0Ni45NDMgODYuNjEzMyAzNDYuOTQzIDg5LjkyQzM0Ni45NDMgOTMuMjggMzQ3LjYwOSA5Ni4yMTMzIDM0OC45NDMgOTguNzJDMzUwLjI3NiAxMDEuMTczIDM1Mi4wNjMgMTAzLjA2NyAzNTQuMzAzIDEwNC40QzM1Ni41NDMgMTA1LjY4IDM1OS4wMjMgMTA2LjMyIDM2MS43NDMgMTA2LjMyQzM2NC40NjMgMTA2LjMyIDM2Ni45NDMgMTA1LjY4IDM2OS4xODMgMTA0LjRDMzcxLjQ3NiAxMDMuMDY3IDM3My4yODkgMTAxLjE3MyAzNzQuNjIzIDk4LjcyQzM3NS45NTYgOTYuMjEzMyAzNzYuNjIzIDkzLjMwNjcgMzc2LjYyMyA5MFoiIGZpbGw9ImJsYWNrIi8+Cjwvc3ZnPgo=";
    
    let profile = match auth_state {
        AuthState::Authenticated { ref profile } => Some(profile.clone()),
        _ => None,
    };
    
    let mut show_dropdown = use_signal(|| false);
    
    rsx! {
        header { id: "app-header",
            div { class: "logo-section",
                img { 
                    src: "{LOGO_SVG}",
                    style: "height: 32px;",
                }
            }
            
            div { class: "header-actions",
                // Calendar icon button
                button {
                    class: if show_calendar_view { "calendar-button active" } else { "calendar-button" },
                    onclick: move |_| on_calendar_click.call(()),
                    title: "View Calendar Events",
                    dangerous_inner_html: r#"<svg width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg"><rect x="3" y="4" width="14" height="13" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/><path d="M3 8h14" stroke="currentColor" stroke-width="1.5"/><path d="M7 2v3M13 2v3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>"#
                }
                
                if let Some(ref p) = profile {
                    div { 
                        class: "user-profile-container",
                        button {
                            class: "user-profile-button",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            div { class: "user-avatar", "{p.initials()}" }
                            div { class: "user-name-compact", "{p.display_name()}" }
                            span { class: "dropdown-arrow", if *show_dropdown.read() { "▲" } else { "▼" } }
                        }
                        
                        if *show_dropdown.read() {
                            div { 
                                class: "dropdown-menu",
                                div { class: "dropdown-item dropdown-header",
                                    div { class: "dropdown-user-name", "{p.display_name()}" }
                                    if !p.email.is_empty() {
                                        div { class: "dropdown-user-email", "{p.email}" }
                                    }
                                }
                                div { class: "dropdown-divider" }
                                button {
                                    class: "dropdown-item dropdown-button",
                                    onclick: move |_| {
                                        let _ = auth::logout();
                                        println!("🔓 Logged out successfully");
                                        std::process::exit(0);
                                    },
                                    "🚪 Logout"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CalendarEventsView(events: Vec<auth::MeetingEvent>, on_close: EventHandler<()>) -> Element {
    let mut current_page = use_signal(|| 0);
    
    const EVENTS_PER_PAGE: usize = 10;
    let total_pages = (events.len() + EVENTS_PER_PAGE - 1) / EVENTS_PER_PAGE;
    let current_page_num = *current_page.read();
    
    let start_idx = current_page_num * EVENTS_PER_PAGE;
    let end_idx = (start_idx + EVENTS_PER_PAGE).min(events.len());
    let page_events: Vec<_> = events.iter().skip(start_idx).take(end_idx - start_idx).collect();
    
    let subtitle = if events.is_empty() {
        "No meetings scheduled".to_string()
    } else if total_pages > 1 {
        format!("Page {} of {}", current_page_num + 1, total_pages)
    } else {
        format!("{} meeting{}", events.len(), if events.len() == 1 { "" } else { "s" })
    };
    
    rsx! {
        div { class: "calendar-overlay",
            div { class: "calendar-container",
                div { class: "calendar-header",
                    div { class: "calendar-header-content",
                        h2 { class: "calendar-title", "Upcoming Meetings" }
                        p { class: "calendar-subtitle", "{subtitle}" }
                    }
                    button {
                        class: "calendar-close-btn",
                        onclick: move |_| on_close.call(()),
                        dangerous_inner_html: r#"<svg width="20" height="20" viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M15 5L5 15M5 5l10 10" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>"#
                    }
                }
                
                div { class: "calendar-content",
                    if events.is_empty() {
                        div { class: "no-events",
                            div { class: "no-events-icon",
                                dangerous_inner_html: r#"<svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg"><rect x="8" y="12" width="32" height="28" rx="3" stroke="currentColor" stroke-width="2" fill="none"/><path d="M8 18h32" stroke="currentColor" stroke-width="2"/><path d="M16 8v6M32 8v6" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><circle cx="16" cy="26" r="1.5" fill="currentColor"/><circle cx="24" cy="26" r="1.5" fill="currentColor"/><circle cx="32" cy="26" r="1.5" fill="currentColor"/></svg>"#
                            }
                            div { class: "no-events-text", "No upcoming meetings scheduled" }
                        }
                    } else {
                        for event in page_events.iter() {
                            div { class: "meeting-card",
                                div { class: "meeting-card-left",
                                    div { class: "meeting-time",
                                        "{event.formatted_start_time()}"
                                    }
                                    div { class: "meeting-title",
                                        "{event.event_summary}"
                                    }
                                }
                                button {
                                    class: "meeting-join-btn",
                                    onclick: {
                                        let url = event.meeting_url.clone();
                                        move |_| {
                                            let _ = std::process::Command::new("open").arg(&url).spawn();
                                        }
                                    },
                                    title: "Open meeting link",
                                    dangerous_inner_html: r#"<svg width="18" height="18" viewBox="0 0 64 64" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M36.026,20.058l-21.092,0c-1.65,0 -2.989,1.339 -2.989,2.989l0,25.964c0,1.65 1.339,2.989 2.989,2.989l26.024,0c1.65,0 2.989,-1.339 2.989,-2.989l0,-20.953l3.999,0l0,21.948c0,3.308 -2.686,5.994 -5.995,5.995l-28.01,0c-3.309,0 -5.995,-2.687 -5.995,-5.995l0,-27.954c0,-3.309 2.686,-5.995 5.995,-5.995l22.085,0l0,4.001Z"/><path d="M55.925,25.32l-4.005,0l0,-10.481l-27.894,27.893l-2.832,-2.832l27.895,-27.895l-10.484,0l0,-4.005l17.318,0l0.002,0.001l0,17.319Z"/></svg>"#
                                }
                            }
                        }
                    }
                }
                
                if total_pages > 1 {
                    div { class: "calendar-pagination",
                        button {
                            class: "pagination-btn",
                            disabled: current_page_num == 0,
                            onclick: move |_| {
                                if current_page_num > 0 {
                                    current_page.set(current_page_num - 1);
                                }
                            },
                            dangerous_inner_html: r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M10 12L6 8l4-4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
                        }
                        
                        div { class: "pagination-info",
                            "Page {current_page_num + 1} of {total_pages}"
                        }
                        
                        button {
                            class: "pagination-btn",
                            disabled: current_page_num >= total_pages - 1,
                            onclick: move |_| {
                                if current_page_num < total_pages - 1 {
                                    current_page.set(current_page_num + 1);
                                }
                            },
                            dangerous_inner_html: r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M6 12l4-4-4-4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NextMeetingNotification(events: Vec<auth::MeetingEvent>) -> Element {
    use chrono::{DateTime, Utc, Duration as ChronoDuration};
    
    // Find the next upcoming meeting (within next 24 hours)
    let now = Utc::now();
    let next_24_hours = now + ChronoDuration::hours(24);
    
    let next_meeting = events.iter().find(|event| {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&event.meeting_start_time) {
            let event_time = dt.with_timezone(&Utc);
            event_time > now && event_time < next_24_hours
        } else {
            false
        }
    });
    
    if let Some(meeting) = next_meeting {
        rsx! {
            div { class: "next-meeting-bar",
                div { class: "next-meeting-content",
                    span { class: "next-meeting-label", "Next Meeting:" }
                    span { class: "next-meeting-time", "{meeting.formatted_start_time()}" }
                    span { class: "next-meeting-title", "- {meeting.event_summary}" }
                }
            }
        }
    } else {
        rsx! { div {} }
    }
}

#[component]
fn MainContent(is_capturing: bool, is_recording: bool, source_name: String, recording_duration: String, upload_status: String, uploaded_file_id: String) -> Element {
    let has_source = !source_name.is_empty() && source_name != "No source selected";
    let show_upload_status = !upload_status.is_empty();
    
    // Determine upload state from status message
    let is_uploading = upload_status.contains("%") || upload_status.contains("Preparing") || upload_status.contains("Processing") || upload_status.contains("Finalizing");
    let is_upload_complete = upload_status.contains("ready") && !uploaded_file_id.is_empty();
    let is_upload_error = upload_status.contains("try again") || upload_status.contains("failed") || upload_status.contains("lost");
    
    // Clone file_id for closures
    let file_id_for_copy = uploaded_file_id.clone();
    let file_id_for_open = uploaded_file_id.clone();
    
    let status_card_class = if is_upload_complete { 
        "status-card success" 
    } else if is_upload_error { 
        "status-card error" 
    } else { 
        "status-card uploading" 
    };
    
    let status_icon = if is_uploading { 
        "" 
    } else if is_upload_complete { 
        "" 
    } else if is_upload_error { 
        "" 
    } else {
        ""
    };
    
    rsx! {
        div { id: "main-content",
            if is_recording {
                // Recording state: Show timer and controls
                div { class: "recording-view",
                    div { class: "timer-container",
                        div { class: "pulse-dot" }
                        div { class: "timer-display", "{recording_duration}" }
                    }
                    div { class: "recording-actions",
                        button {
                            class: "btn btn-danger btn-large",
                            onclick: move |_| {
                                let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                                if let Some(ref sender) = tx {
                                    let _ = sender.send(CaptureCommand::StopRecording);
                                }
                            },
                            "Stop & Upload"
                        }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                                if let Some(ref sender) = tx {
                                    // Cancel recording - will delete file and not upload
                                    let _ = sender.send(CaptureCommand::CancelRecording);
                                }
                            },
                            "Cancel"
                        }
                    }
                }
            } else if show_upload_status {
                // Upload status state
                div { class: "status-view",
                    div { 
                        class: "{status_card_class}",
                        // Progress indicator for uploading
                        if is_uploading {
                            div { class: "upload-progress",
                                div { class: "progress-spinner" }
                                div { class: "progress-ring-container",
                                    svg { 
                                        class: "progress-ring",
                                        width: "60",
                                        height: "60",
                                        circle {
                                            class: "progress-ring-circle",
                                            stroke: "currentColor",
                                            "stroke-width": "3",
                                            fill: "transparent",
                                            r: "26",
                                            cx: "30",
                                            cy: "30",
                                        }
                                    }
                                }
                            }
                        }
                        
                        div { class: "status-title",
                            if is_uploading { "Uploading Recording" }
                            else if is_upload_complete { "Upload Complete" }
                            else if is_upload_error { "Upload Failed" }
                            else { "Processing" }
                        }
                        div { class: "status-message", "{upload_status}" }
                        
                        // Show URL and actions when complete
                        if is_upload_complete {
                            div { class: "recording-url-section",
                                div { class: "url-label", "Recording URL" }
                                div { class: "url-box-container",
                                    div { class: "url-box",
                                        input {
                                            class: "url-input",
                                            readonly: true,
                                            value: "https://insights.talka.ai/activity/meeting?fileId={uploaded_file_id}",
                                        }
                                    }
                                    button {
                                        class: "btn btn-icon btn-secondary copy-btn-inline",
                                        title: "Copy link to clipboard",
                                        onclick: move |_| {
                                            let url = format!("https://insights.talka.ai/activity/meeting?fileId={}", file_id_for_copy);
                                            let _ = copy_to_clipboard(&url);
                                        },
                                        dangerous_inner_html: r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M10.5 2H3.5C2.67 2 2 2.67 2 3.5V11.5C2 11.78 2.22 12 2.5 12C2.78 12 3 11.78 3 11.5V3.5C3 3.22 3.22 3 3.5 3H10.5C10.78 3 11 2.78 11 2.5C11 2.22 10.78 2 10.5 2ZM12.5 4H5.5C4.67 4 4 4.67 4 5.5V12.5C4 13.33 4.67 14 5.5 14H12.5C13.33 14 14 13.33 14 12.5V5.5C14 4.67 13.33 4 12.5 4ZM12.5 12.5H5.5V5.5H12.5V12.5Z" fill="currentColor"/></svg>"#
                                    }
                                }
                                div { class: "url-actions-buttons",
                                    button {
                                        class: "btn btn-primary btn-action",
                                        onclick: move |_| {
                                            let url = format!("https://insights.talka.ai/activity/meeting?fileId={}", file_id_for_open);
                                            let _ = std::process::Command::new("open").arg(&url).spawn();
                                        },
                                        "Open Meeting"
                                    }
                                    button {
                                        class: "btn btn-secondary btn-action",
                                        onclick: move |_| {
                                            let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                                            if let Some(ref sender) = tx {
                                                let _ = sender.send(CaptureCommand::SelectSource);
                                            }
                                        },
                                        "New Recording"
                                    }
                                }
                            }
                        }
                    }
                    
                    if is_upload_error {
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                                if let Some(ref sender) = tx {
                                    let _ = sender.send(CaptureCommand::SelectSource);
                                }
                            },
                            "New Recording"
                        }
                    }
                }
            } else if !has_source {
                // No source state: Show select source
                div { class: "welcome-view",
                    h2 { class: "welcome-title", "Select Source to Record" }
                    p { class: "welcome-subtitle", "Choose your screen or window to start recording" }
                    button {
                        class: "btn btn-primary btn-hero",
                        onclick: move |_| {
                            let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                            if let Some(ref sender) = tx {
                                let _ = sender.send(CaptureCommand::SelectSource);
                            }
                        },
                        "Select Source"
                    }
                }
            } else {
                // Source selected state: Show ready to record
                div { class: "ready-view",
                    div { class: "source-info-card",
                        div { class: "source-label", "Source" }
                        div { class: "source-name-display", "{source_name}" }
                    }
                    button {
                        class: "btn btn-success btn-hero",
                        onclick: move |_| {
                            let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                            if let Some(ref sender) = tx {
                                let _ = sender.send(CaptureCommand::StartRecording);
                            }
                        },
                        disabled: !is_capturing,
                        "Start Recording"
                    }
                    button {
                        class: "btn btn-text",
                        onclick: move |_| {
                            let (tx, _, _, _, _, _, _, _, _, _) = get_global_state();
                            if let Some(ref sender) = tx {
                                // Stop sharing the current source first
                                let _ = sender.send(CaptureCommand::StopCapture);
                            }
                        },
                        "Stop Sharing"
                    }
                }
            }
        }
    }
}


// Capture backend thread
fn run_capture_backend(
    cmd_rx: Receiver<CaptureCommand>,
    is_capturing: Arc<AtomicBool>,
    is_recording: Arc<AtomicBool>,
    source_name: Arc<Mutex<String>>,
    upload_status: Arc<Mutex<String>>,
    uploaded_file_id: Arc<Mutex<String>>,
    runtime: tokio::runtime::Handle,
    capture_state: Arc<CaptureState>,
    auth_tokens: Arc<Mutex<Option<auth::AuthTokens>>>,
) {
    let mut stream: Option<SCStream> = None;
    let mut current_filter: Option<SCContentFilter> = None;
    let stream_config = default_stream_config();
    let mut capture_size = (1280u32, 720u32);
    let pending_picker: Arc<Mutex<PickerResult>> = Arc::new(Mutex::new(None));

    #[cfg(feature = "macos_15_0")]
    let mut recording_state = RecordingState::new();
    #[cfg(feature = "macos_15_0")]
    let recording_config = RecordingConfig::new();

    loop {
        // First check for pending picker results (continuously polling)
        if let Ok(mut pending) = pending_picker.try_lock() {
            if let Some((filter, width, height, source)) = pending.take() {
                // Update source info immediately
                let source_display = format_picked_source(&source);
                *source_name.lock().unwrap() = source_display.clone();
                println!("✅ Source selected: {}", source_display);
                
                // If already capturing, update the filter live
                if is_capturing.load(Ordering::Relaxed) {
                    if let Some(ref s) = stream {
                        let _ = s.update_content_filter(&filter);
                        println!("🔄 Updated capture filter to new source");
                    }
                } else {
                    // Store filter and size for future capture
                    current_filter = Some(filter.clone());
                    capture_size = (width, height);
                    
                    // Auto-start capture after picking (like original app)
                    input::start_capture(
                        &mut stream,
                        Some(&filter),
                        capture_size,
                        &stream_config,
                        &capture_state,
                        &is_capturing,
                        false,
                    );
                }
            }
        }
        
        // Then check for commands (with timeout to continue polling)
        if let Ok(cmd) = cmd_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            match cmd {
                CaptureCommand::SelectSource => {
                    // Clear previous upload status and file ID
                    *upload_status.lock().unwrap() = String::new();
                    *uploaded_file_id.lock().unwrap() = String::new();
                    
                    // Open picker (result will be handled in the polling loop above)
                    if let Some(ref s) = stream {
                        input::open_picker_for_stream(&pending_picker, s);
                    } else {
                        input::open_picker(&pending_picker);
                    }
                    println!("📺 Opening content picker...");
                }
                CaptureCommand::StartCapture => {
                    if current_filter.is_some() {
                        input::start_capture(
                            &mut stream,
                            current_filter.as_ref(),
                            capture_size,
                            &stream_config,
                            &capture_state,
                            &is_capturing,
                            false,
                        );
                    } else {
                        println!("⚠️ No source selected. Please select a source first.");
                    }
                }
                CaptureCommand::StopCapture => {
                    // Stop capture and clear source
                    input::stop_capture(&mut stream, &is_capturing);
                    *source_name.lock().unwrap() = "No source selected".to_string();
                    current_filter = None;
                    println!("🔄 Source detached, ready to select new source");
                }
                CaptureCommand::TakeScreenshot => {
                    if is_capturing.load(Ordering::Relaxed) {
                        println!("📸 Taking screenshot...");
                        // Screenshot logic would go here
                    }
                }
                CaptureCommand::StartRecording => {
                    #[cfg(feature = "macos_15_0")]
                    if is_capturing.load(Ordering::Relaxed) {
                        if let Some(ref s) = stream {
                            match recording_state.start(s, &recording_config) {
                                Ok(path) => {
                                    is_recording.store(true, Ordering::Relaxed);
                                    println!("⏺ Recording started: {}", path);
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to start recording: {}", e);
                                }
                            }
                        }
                    }
                }
                CaptureCommand::StopRecording => {
                    #[cfg(feature = "macos_15_0")]
                    {
                        if let Some(ref s) = stream {
                            println!("⏹ Stopping recording...");
                            if let Some(path) = recording_state.stop(s) {
                                is_recording.store(false, Ordering::Relaxed);
                                println!("✅ Recording stopped and saved: {}", path);
                                
                                // Stop capture and clear source after recording
                                input::stop_capture(&mut stream, &is_capturing);
                                *source_name.lock().unwrap() = "No source selected".to_string();
                                current_filter = None;
                                println!("🔄 Source cleared, ready for next recording");
                                
                                // Trigger upload to Talka backend
                                let tokens_opt = auth_tokens.lock().unwrap().clone();
                                if let Some(tokens) = tokens_opt {
                                    println!("🚀 Starting upload to Talka backend...");
                                    *upload_status.lock().unwrap() = "Preparing your recording".to_string();
                                    
                                    let runtime_clone = runtime.clone();
                                    let recording_state_clone = recording_state.clone();
                                    let upload_status_clone = Arc::clone(&upload_status);
                                    let uploaded_file_id_clone = Arc::clone(&uploaded_file_id);
                                    
                                    runtime.spawn(async move {
                                        // Refresh access token if needed
                                        let access_token = if tokens.is_expired() {
                                            println!("🔄 Refreshing access token...");
                                            match auth::refresh_access_token(&tokens.refresh_token).await {
                                                Ok(new_tokens) => {
                                                    println!("✅ Token refreshed");
                                                    let _ = auth::save_tokens(&new_tokens);
                                                    new_tokens.access_token
                                                }
                                                Err(e) => {
                                                    println!("⚠️ Token refresh failed: {}, using old token", e);
                                                    tokens.access_token
                                                }
                                            }
                                        } else {
                                            tokens.access_token
                                        };
                                        
                                        // Start upload with status updates
                                        println!("📤 Uploading file: {}", path);
                                        recording_state_clone.start_upload(
                                            path,
                                            access_token,
                                            runtime_clone,
                                        );
                                        
                                        // Monitor upload status and update UI
                                        loop {
                                            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                                            let current_status = recording_state_clone.upload_status.lock().unwrap().clone();
                                            
                                            let status_text = current_status.as_display_string();
                                            if !status_text.is_empty() {
                                                *upload_status_clone.lock().unwrap() = status_text.clone();
                                            }
                                            
                                            // Extract and store file_id when complete
                                            if let upload::UploadStatus::Complete { ref file_id } = current_status {
                                                *uploaded_file_id_clone.lock().unwrap() = file_id.clone();
                                            }
                                            
                                            // Stop monitoring if complete or failed
                                            if matches!(current_status, upload::UploadStatus::Complete { .. } | upload::UploadStatus::Failed(_)) {
                                                println!("📊 Upload finished: {:?}", current_status);
                                                // Keep status displayed until user takes action
                                                break;
                                            }
                                        }
                                    });
                                } else {
                                    println!("⚠️ No authentication tokens available for upload");
                                    *upload_status.lock().unwrap() = "Please log in to upload recordings".to_string();
                                    std::thread::sleep(std::time::Duration::from_secs(3));
                                    *upload_status.lock().unwrap() = String::new();
                                }
                            } else {
                                println!("⚠️ No recording to stop");
                            }
                        } else {
                            println!("⚠️ No active stream");
                        }
                    }
                    #[cfg(not(feature = "macos_15_0"))]
                    {
                        println!("⚠️ Recording not available (requires macOS 15.0+)");
                    }
                }
                CaptureCommand::CancelRecording => {
                    #[cfg(feature = "macos_15_0")]
                    {
                        if let Some(ref s) = stream {
                            println!("✖️ Canceling recording...");
                            if let Some(path) = recording_state.stop(s) {
                                is_recording.store(false, Ordering::Relaxed);
                                println!("🗑️ Deleting recording: {}", path);
                                
                                // Delete the recorded file
                                if let Err(e) = std::fs::remove_file(&path) {
                                    eprintln!("⚠️ Failed to delete recording file: {}", e);
                                } else {
                                    println!("✅ Recording file deleted");
                                }
                                
                                // Stop capture and clear source
                                input::stop_capture(&mut stream, &is_capturing);
                                *source_name.lock().unwrap() = "No source selected".to_string();
                                current_filter = None;
                                println!("🔄 Source cleared, ready for next recording");
                                
                                // Clear upload status (no upload on cancel)
                                *upload_status.lock().unwrap() = String::new();
                            } else {
                                println!("⚠️ No recording to cancel");
                            }
                        } else {
                            println!("⚠️ No active stream");
                        }
                    }
                    #[cfg(not(feature = "macos_15_0"))]
                    {
                        println!("⚠️ Recording not available (requires macOS 15.0+)");
                    }
                }
                CaptureCommand::ToggleMicrophone => {
                    println!("🎤 Toggle microphone");
                }
                CaptureCommand::Quit => {
                    break;
                }
                CaptureCommand::Logout => {
                    break;
                }
            }
        }
    }
}

/// Helper to copy text to clipboard (macOS specific)
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::process::Command;
    
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;
    
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())
            .map_err(|e| format!("Failed to write to pbcopy: {}", e))?;
    }
    
    child.wait().map_err(|e| format!("Failed to wait for pbcopy: {}", e))?;
    println!("📋 Copied to clipboard: {}", text);
    Ok(())
}

async fn authenticate_user_with_ui(auth_state: &Arc<Mutex<AuthState>>) -> Result<(auth::AuthTokens, auth::UserProfile), String> {
    // Try to load existing tokens and validate with profile fetch
    if let Some(cached_tokens) = auth::load_tokens() {
        if !cached_tokens.is_expired() {
            // Try to fetch profile to validate token
            match auth::get_user_profile(&cached_tokens.access_token).await {
                Ok(profile) => {
                    println!("✅ Loaded cached tokens and profile");
                    return Ok((cached_tokens, profile));
                }
                Err(_) => {
                    println!("⚠️ Cached token invalid, refreshing...");
                }
            }
        }
        
        // Try to refresh
        if !cached_tokens.refresh_token.is_empty() {
            match auth::refresh_access_token(&cached_tokens.refresh_token).await {
                Ok(new_tokens) => {
                    let _ = auth::save_tokens(&new_tokens);
                    // Fetch profile
                    match auth::get_user_profile(&new_tokens.access_token).await {
                        Ok(profile) => {
                            println!("✅ Refreshed tokens and fetched profile");
                            return Ok((new_tokens, profile));
                        }
                        Err(_) => {
                            println!("⚠️ Failed to fetch profile after refresh");
                        }
                    }
                }
                Err(_) => {
                    println!("⚠️ Token refresh failed, need new login");
                }
            }
        }
    }

    // Start device flow
    let (verification_uri, user_code, device_response) = auth::start_device_flow()
        .await
        .map_err(|e| format!("Failed to start auth: {}", e))?;

    println!("🔐 Please authenticate:");
    println!("   URL: {}", verification_uri);
    println!("   Code: {}", user_code);

    // Update UI state to show login screen
    *auth_state.lock().unwrap() = AuthState::NeedsAuth {
        verification_uri: verification_uri.clone(),
        user_code: user_code.clone(),
    };

    // Poll for completion
    let start_time = std::time::Instant::now();
    let expires_at = start_time + std::time::Duration::from_secs(device_response.expires_in);
    let mut poll_interval = std::time::Duration::from_secs(device_response.interval);

    loop {
        if std::time::Instant::now() >= expires_at {
            return Err("Device code expired".to_string());
        }

        tokio::time::sleep(poll_interval).await;

        match auth::poll_for_token(&device_response.device_code).await {
            Ok(mut tokens) => {
                tokens.update_expiration();
                let _ = auth::save_tokens(&tokens);
                
                // Update UI state
                *auth_state.lock().unwrap() = AuthState::Authenticating;
                
                // Fetch user profile
                match auth::get_user_profile(&tokens.access_token).await {
                    Ok(profile) => {
                        println!("✅ Authentication complete!");
                        return Ok((tokens, profile));
                    }
                    Err(e) => {
                        return Err(format!("Failed to fetch user profile: {}", e));
                    }
                }
            }
            Err(auth::AuthError::AuthorizationPending) => {
                // Keep waiting
            }
            Err(auth::AuthError::SlowDown) => {
                poll_interval += std::time::Duration::from_secs(5);
            }
            Err(e) => {
                return Err(format!("Auth failed: {}", e));
            }
        }
    }
}
