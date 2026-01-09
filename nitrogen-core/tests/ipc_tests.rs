//! Integration tests for IPC protocol

use nitrogen_core::ipc::{IpcMessage, IpcResponse, PipelineStatistics, PipelineStatus};

#[test]
fn test_message_ping_serialization() {
    let msg = IpcMessage::Ping;
    let bytes = msg.to_bytes();
    let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcMessage::Ping));
}

#[test]
fn test_message_status_serialization() {
    let msg = IpcMessage::Status;
    let bytes = msg.to_bytes();
    let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcMessage::Status));
}

#[test]
fn test_message_stats_serialization() {
    let msg = IpcMessage::Stats;
    let bytes = msg.to_bytes();
    let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcMessage::Stats));
}

#[test]
fn test_message_stop_serialization() {
    let msg = IpcMessage::Stop;
    let bytes = msg.to_bytes();
    let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcMessage::Stop));
}

#[test]
fn test_message_force_stop_serialization() {
    let msg = IpcMessage::ForceStop;
    let bytes = msg.to_bytes();
    let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcMessage::ForceStop));
}

#[test]
fn test_response_ok_serialization() {
    let resp = IpcResponse::Ok;
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcResponse::Ok));
}

#[test]
fn test_response_pong_serialization() {
    let resp = IpcResponse::Pong;
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcResponse::Pong));
}

#[test]
fn test_response_error_serialization() {
    let resp = IpcResponse::error("Test error message");
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    match parsed {
        IpcResponse::Error { message } => assert_eq!(message, "Test error message"),
        _ => panic!("Expected Error response"),
    }
}

#[test]
fn test_response_stopping_serialization() {
    let resp = IpcResponse::Stopping;
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    assert!(matches!(parsed, IpcResponse::Stopping));
}

#[test]
fn test_response_status_serialization() {
    let status = PipelineStatus {
        running: true,
        state: "Running".to_string(),
        source: Some("monitor:DP-1".to_string()),
        resolution: Some((1920, 1080)),
        fps: Some(60),
        camera_name: Some("Test Camera".to_string()),
        pid: 12345,
        uptime_seconds: 123.45,
    };
    let resp = IpcResponse::Status(status);
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    match parsed {
        IpcResponse::Status(s) => {
            assert!(s.running);
            assert_eq!(s.state, "Running");
            assert_eq!(s.source, Some("monitor:DP-1".to_string()));
            assert_eq!(s.resolution, Some((1920, 1080)));
            assert_eq!(s.fps, Some(60));
            assert_eq!(s.camera_name, Some("Test Camera".to_string()));
            assert_eq!(s.pid, 12345);
            assert!((s.uptime_seconds - 123.45).abs() < 0.001);
        }
        _ => panic!("Expected Status response"),
    }
}

#[test]
fn test_response_stats_serialization() {
    let stats = PipelineStatistics {
        frames_processed: 1000,
        frames_dropped: 5,
        frames_failed: 1,
        actual_fps: 59.94,
        target_fps: 60,
        elapsed_seconds: 16.67,
        resolution: (1920, 1080),
        codec: "H.264".to_string(),
        bitrate: 6000,
    };
    let resp = IpcResponse::Stats(stats);
    let bytes = resp.to_bytes();
    let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).expect("Should parse");
    match parsed {
        IpcResponse::Stats(s) => {
            assert_eq!(s.frames_processed, 1000);
            assert_eq!(s.frames_dropped, 5);
            assert_eq!(s.frames_failed, 1);
            assert!((s.actual_fps - 59.94).abs() < 0.001);
            assert_eq!(s.target_fps, 60);
            assert!((s.elapsed_seconds - 16.67).abs() < 0.001);
            assert_eq!(s.resolution, (1920, 1080));
            assert_eq!(s.codec, "H.264");
            assert_eq!(s.bitrate, 6000);
        }
        _ => panic!("Expected Stats response"),
    }
}

#[test]
fn test_invalid_message_parsing() {
    let result = IpcMessage::from_bytes(b"not valid json");
    assert!(result.is_err());
}

#[test]
fn test_invalid_response_parsing() {
    let result = IpcResponse::from_bytes(b"not valid json");
    assert!(result.is_err());
}

#[test]
fn test_message_json_format() {
    let msg = IpcMessage::Status;
    let bytes = msg.to_bytes();
    let json_str = std::str::from_utf8(&bytes[..bytes.len() - 1]).expect("Should be valid UTF-8");
    assert!(json_str.contains("\"type\":\"Status\""));
}

#[test]
fn test_response_json_format() {
    let resp = IpcResponse::error("test");
    let bytes = resp.to_bytes();
    let json_str = std::str::from_utf8(&bytes[..bytes.len() - 1]).expect("Should be valid UTF-8");
    assert!(json_str.contains("\"type\":\"Error\""));
    assert!(json_str.contains("\"message\":\"test\""));
}

#[test]
fn test_bytes_have_newline_terminator() {
    let msg = IpcMessage::Ping;
    let bytes = msg.to_bytes();
    assert_eq!(bytes.last(), Some(&b'\n'));

    let resp = IpcResponse::Pong;
    let bytes = resp.to_bytes();
    assert_eq!(bytes.last(), Some(&b'\n'));
}
