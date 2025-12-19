//! Modern Dioxus UI for Screen Capture Application - Type Definitions

#[derive(Clone, Debug)]
pub enum CaptureCommand {
    StartCapture,
    StopCapture,
    TakeScreenshot,
    StartRecording,
    StopRecording,
    SelectSource,
    ToggleMicrophone,
    Quit,
    Logout,
}
