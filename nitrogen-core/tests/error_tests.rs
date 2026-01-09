//! Integration tests for error handling

use nitrogen_core::error::{NitrogenError, ResultExt};

#[test]
fn test_error_context_chaining() {
    let base_error = NitrogenError::encoder("Codec not found");
    let with_context = base_error.with_context("Failed to initialize encoder");

    let msg = format!("{}", with_context);
    assert!(msg.contains("Failed to initialize encoder"));
    assert!(msg.contains("Codec not found"));
}

#[test]
fn test_error_context_preserves_hint() {
    let base_error = NitrogenError::nvenc("GPU not available");
    let hint_before = base_error.user_hint();

    let with_context = base_error.with_context("During encoding");
    let hint_after = with_context.user_hint();

    // Hint should be preserved through context
    assert_eq!(hint_before, hint_after);
}

#[test]
fn test_result_ext_context() {
    let result: Result<(), NitrogenError> = Err(NitrogenError::portal("Connection failed"));
    let with_context = result.context("Starting capture session");

    assert!(with_context.is_err());
    let err = with_context.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Starting capture session"));
}

#[test]
fn test_user_hints() {
    // Portal errors should have hints
    let err = NitrogenError::portal("test");
    assert!(err.user_hint().is_some());
    assert!(err.user_hint().unwrap().contains("xdg-desktop-portal"));

    // PipeWire errors should have hints
    let err = NitrogenError::pipewire("test");
    assert!(err.user_hint().is_some());
    assert!(err.user_hint().unwrap().contains("PipeWire"));

    // NVENC errors should have hints
    let err = NitrogenError::nvenc("test");
    assert!(err.user_hint().is_some());
    assert!(err.user_hint().unwrap().contains("NVIDIA"));

    // Config errors should have hints
    let err = NitrogenError::config("test");
    assert!(err.user_hint().is_some());
    assert!(err.user_hint().unwrap().contains("config.toml"));
}

#[test]
fn test_user_recoverable() {
    // These should be user-recoverable
    assert!(NitrogenError::portal("test").is_user_recoverable());
    assert!(NitrogenError::pipewire("test").is_user_recoverable());
    assert!(NitrogenError::config("test").is_user_recoverable());
    assert!(NitrogenError::SourceNotFound("test".to_string()).is_user_recoverable());
    assert!(NitrogenError::NoActiveSession.is_user_recoverable());
    assert!(NitrogenError::SessionAlreadyRunning.is_user_recoverable());

    // These should not be user-recoverable (require code changes or unusual conditions)
    assert!(!NitrogenError::encoder("test").is_user_recoverable());
    assert!(!NitrogenError::nvenc("test").is_user_recoverable());
    assert!(!NitrogenError::Unsupported("test".to_string()).is_user_recoverable());
}

#[test]
fn test_error_display_format() {
    let err = NitrogenError::portal("Connection refused");
    assert_eq!(format!("{}", err), "Portal error: Connection refused");

    let err = NitrogenError::encoder("Invalid codec");
    assert_eq!(format!("{}", err), "Encoder error: Invalid codec");

    let err = NitrogenError::NoActiveSession;
    assert_eq!(format!("{}", err), "No active capture session");

    let err = NitrogenError::SessionAlreadyRunning;
    assert_eq!(format!("{}", err), "Capture session already running");
}

#[test]
fn test_nested_context() {
    let err = NitrogenError::pipewire("Stream failed")
        .with_context("Processing frame")
        .with_context("During capture");

    let msg = format!("{}", err);
    assert!(msg.contains("During capture"));
    // The full chain should still preserve the original hint
    assert!(err.user_hint().is_some());
}

#[test]
fn test_io_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
    let nitrogen_err: NitrogenError = io_err.into();

    let msg = format!("{}", nitrogen_err);
    assert!(msg.contains("I/O error"));
    assert!(msg.contains("File not found"));
}

#[test]
fn test_source_not_found_hint() {
    let err = NitrogenError::SourceNotFound("DP-99".to_string());
    let hint = err.user_hint();

    assert!(hint.is_some());
    assert!(hint.unwrap().contains("nitrogen list"));
}

#[test]
fn test_session_already_running_hint() {
    let err = NitrogenError::SessionAlreadyRunning;
    let hint = err.user_hint();

    assert!(hint.is_some());
    assert!(hint.unwrap().contains("nitrogen stop"));
}
