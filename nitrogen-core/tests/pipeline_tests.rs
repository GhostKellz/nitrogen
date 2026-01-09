//! Integration tests for the capture pipeline
//!
//! Note: Tests that require actual PipeWire/NVENC are marked with #[ignore].
//! Run with `cargo test -- --ignored` to include them.

mod mocks;

use nitrogen_core::config::{CaptureConfig, Codec, EncoderPreset, Preset};
use nitrogen_core::types::FrameData;

#[test]
fn test_capture_config_for_discord_1440p60() {
    // The main use case: 1440p60 for Discord
    let config = CaptureConfig::monitor("portal")
        .with_preset(Preset::P1440_60)
        .with_codec(Codec::H264)
        .with_encoder_preset(EncoderPreset::Fast);

    let (width, height) = config.preset.resolution();
    assert_eq!(width, 2560);
    assert_eq!(height, 1440);
    assert_eq!(config.preset.fps(), 60);

    // Bitrate should be reasonable for 1440p60
    let bitrate = config.effective_bitrate();
    assert!(bitrate >= 8000); // At least 8 Mbps
    assert!(bitrate <= 20000); // Not excessive

    // Validation should pass
    assert!(config.validate_strict().is_ok());
    // Note: 1440p60 is a common use case, validation may or may not warn
    // depending on implementation. We just verify it validates successfully.
}

#[test]
fn test_capture_config_for_discord_1440p30() {
    let config = CaptureConfig::monitor("portal")
        .with_preset(Preset::P1440_30)
        .with_codec(Codec::H264);

    let (width, height) = config.preset.resolution();
    assert_eq!(width, 2560);
    assert_eq!(height, 1440);
    assert_eq!(config.preset.fps(), 30);

    // 30fps should have lower bitrate than 60fps
    let bitrate = config.effective_bitrate();
    assert!(bitrate >= 4000);
    assert!(bitrate <= 12000);
}

#[test]
fn test_capture_config_for_discord_1080p60() {
    let config = CaptureConfig::monitor("portal")
        .with_preset(Preset::P1080_60)
        .with_codec(Codec::H264);

    let (width, height) = config.preset.resolution();
    assert_eq!(width, 1920);
    assert_eq!(height, 1080);
    assert_eq!(config.preset.fps(), 60);

    // Should be within Discord's advertised limits
    let warnings = config.validate();
    // 1080p60 should not trigger Discord warnings (it's within their limits)
    assert!(!warnings
        .iter()
        .any(|w| w.contains("Discord") && w.contains("720p")));
}

#[test]
fn test_capture_config_with_recording() {
    let config = CaptureConfig::monitor("portal")
        .with_preset(Preset::P1080_60)
        .with_record_path("/tmp/test.mp4");

    assert!(config.record_path.is_some());
    assert_eq!(
        config.record_path.as_ref().map(|p| p.to_str().unwrap()),
        Some("/tmp/test.mp4")
    );
}

#[test]
fn test_all_preset_bitrates_reasonable() {
    let presets = [
        Preset::P720_30,
        Preset::P720_60,
        Preset::P1080_30,
        Preset::P1080_60,
        Preset::P1440_30,
        Preset::P1440_60,
        Preset::P1440_120,
        Preset::P4k30,
        Preset::P4k60,
        Preset::P4k120,
    ];

    for preset in presets {
        let bitrate = preset.suggested_bitrate();
        let (width, height) = preset.resolution();
        let fps = preset.fps();

        // Bitrate should be positive and reasonable
        assert!(
            bitrate > 0,
            "Preset {:?} should have positive bitrate",
            preset
        );
        assert!(
            bitrate <= 100_000,
            "Preset {:?} bitrate {} seems unreasonably high",
            preset,
            bitrate
        );

        // Higher resolution/fps should generally have higher bitrate
        // Just verify we're in a sensible range for each tier
        if width <= 1280 {
            assert!(
                bitrate >= 2000 && bitrate <= 10000,
                "720p bitrate should be 2-10 Mbps"
            );
        } else if width <= 1920 {
            assert!(
                bitrate >= 4000 && bitrate <= 15000,
                "1080p bitrate should be 4-15 Mbps"
            );
        } else if width <= 2560 {
            assert!(
                bitrate >= 6000 && bitrate <= 30000,
                "1440p bitrate should be 6-30 Mbps"
            );
        } else {
            assert!(bitrate >= 10000, "4K bitrate should be at least 10 Mbps");
        }

        eprintln!(
            "Preset {:?}: {}x{} @ {}fps = {} kbps",
            preset, width, height, fps, bitrate
        );
    }
}

#[test]
fn test_mock_frame_creation() {
    let frame = mocks::create_test_frame(1920, 1080, [0, 0, 255, 255]); // Red in BGRA

    assert_eq!(frame.format.width, 1920);
    assert_eq!(frame.format.height, 1080);
    assert_eq!(frame.format.stride, 1920 * 4);

    if let FrameData::Memory(data) = &frame.data {
        // Check first pixel is red (BGRA format)
        assert_eq!(&data[0..4], &[0, 0, 255, 255]);
    } else {
        panic!("Expected Memory frame data");
    }
}

#[test]
fn test_mock_frame_source_broadcast() {
    let source = mocks::MockFrameSource::new();

    let mut rx1 = source.subscribe();
    let mut rx2 = source.subscribe();

    assert_eq!(source.receiver_count(), 2);

    let frame = mocks::create_test_frame(100, 100, [255, 255, 255, 255]);
    let sent = source.send_frame(frame).expect("Should send");
    assert_eq!(sent, 2);

    // Both receivers should get the frame
    assert!(rx1.try_recv().is_ok());
    assert!(rx2.try_recv().is_ok());
}

#[test]
fn test_codec_recommendations() {
    // H.264 should be recommended for maximum compatibility
    let config = CaptureConfig::monitor("test").with_codec(Codec::H264);
    let warnings = config.validate();
    // H.264 shouldn't generate codec-related warnings for normal presets
    assert!(!warnings
        .iter()
        .any(|w| w.contains("codec") && w.contains("compatible")));

    // AV1 might warn about compatibility
    let config = CaptureConfig::monitor("test").with_codec(Codec::Av1);
    // AV1 is fine, just newer - no specific warnings expected in current impl
    let _ = config.validate();
}

#[test]
fn test_encoder_preset_quality_tradeoff() {
    // Fast preset for low latency streaming
    let fast_preset = EncoderPreset::Fast.nvenc_preset();
    assert_eq!(fast_preset, "p1");

    // Quality preset for recording
    let quality_preset = EncoderPreset::Quality.nvenc_preset();
    assert_eq!(quality_preset, "p7");

    // Higher preset numbers = more quality (slower)
    let fast_num: i32 = fast_preset.trim_start_matches('p').parse().unwrap();
    let quality_num: i32 = quality_preset.trim_start_matches('p').parse().unwrap();
    assert!(quality_num > fast_num);
}

// Tests that require actual hardware - run with --ignored

#[test]
#[ignore = "Requires NVENC hardware"]
fn test_nvenc_encoder_creation() {
    use nitrogen_core::encode;

    if !encode::nvenc_available() {
        eprintln!("NVENC not available, skipping");
        return;
    }

    let encoders = encode::available_encoders();
    assert!(!encoders.is_empty(), "Should have at least one encoder");
    eprintln!("Available encoders: {:?}", encoders);
}

#[test]
#[ignore = "Requires NVENC hardware"]
fn test_codec_availability() {
    use nitrogen_core::encode;

    // At minimum, H.264 should be available on NVENC systems
    if encode::nvenc_available() {
        assert!(
            encode::codec_available(Codec::H264),
            "H.264 should be available on NVENC systems"
        );
    }
}

#[test]
#[ignore = "Requires PipeWire and portal"]
fn test_portal_source_listing() {
    use nitrogen_core::capture;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let sources = rt
        .block_on(capture::list_sources())
        .expect("Should list sources");

    // On Wayland, we get placeholder sources
    assert!(!sources.is_empty(), "Should return some sources");
}
