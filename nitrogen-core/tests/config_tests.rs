//! Integration tests for configuration system

use nitrogen_core::config::{
    sample_config, CaptureConfig, Codec, ConfigFile, EncoderPreset, Preset,
};
use tempfile::TempDir;

#[test]
fn test_preset_resolution() {
    assert_eq!(Preset::P720_30.resolution(), (1280, 720));
    assert_eq!(Preset::P720_60.resolution(), (1280, 720));
    assert_eq!(Preset::P1080_30.resolution(), (1920, 1080));
    assert_eq!(Preset::P1080_60.resolution(), (1920, 1080));
    assert_eq!(Preset::P1440_60.resolution(), (2560, 1440));
    assert_eq!(Preset::P4k60.resolution(), (3840, 2160));
}

#[test]
fn test_preset_fps() {
    assert_eq!(Preset::P720_30.fps(), 30);
    assert_eq!(Preset::P720_60.fps(), 60);
    assert_eq!(Preset::P1080_30.fps(), 30);
    assert_eq!(Preset::P1080_60.fps(), 60);
    assert_eq!(Preset::P1440_120.fps(), 120);
    assert_eq!(Preset::P4k120.fps(), 120);
}

#[test]
fn test_preset_from_string() {
    assert_eq!(Preset::from_preset_str("720p30"), Some(Preset::P720_30));
    assert_eq!(Preset::from_preset_str("1080p60"), Some(Preset::P1080_60));
    assert_eq!(Preset::from_preset_str("4k60"), Some(Preset::P4k60));
    assert_eq!(Preset::from_preset_str("2160p60"), Some(Preset::P4k60));
    assert_eq!(Preset::from_preset_str("invalid"), None);
}

#[test]
fn test_custom_preset() {
    let custom = Preset::Custom {
        width: 2560,
        height: 1600,
        fps: 100,
    };
    assert_eq!(custom.resolution(), (2560, 1600));
    assert_eq!(custom.fps(), 100);
}

#[test]
fn test_codec_nvenc_encoder() {
    assert_eq!(Codec::H264.nvenc_encoder(), "h264_nvenc");
    assert_eq!(Codec::Hevc.nvenc_encoder(), "hevc_nvenc");
    assert_eq!(Codec::Av1.nvenc_encoder(), "av1_nvenc");
}

#[test]
fn test_codec_from_string() {
    assert_eq!("h264".parse::<Codec>().ok(), Some(Codec::H264));
    assert_eq!("hevc".parse::<Codec>().ok(), Some(Codec::Hevc));
    assert_eq!("h265".parse::<Codec>().ok(), Some(Codec::Hevc));
    assert_eq!("av1".parse::<Codec>().ok(), Some(Codec::Av1));
    assert!("invalid".parse::<Codec>().is_err());
}

#[test]
fn test_encoder_preset_nvenc() {
    assert_eq!(EncoderPreset::Fast.nvenc_preset(), "p1");
    assert_eq!(EncoderPreset::Medium.nvenc_preset(), "p4");
    assert_eq!(EncoderPreset::Slow.nvenc_preset(), "p6");
    assert_eq!(EncoderPreset::Quality.nvenc_preset(), "p7");
}

#[test]
fn test_capture_config_builder() {
    let config = CaptureConfig::monitor("test")
        .with_preset(Preset::P720_60)
        .with_codec(Codec::Hevc)
        .with_bitrate(5000)
        .with_encoder_preset(EncoderPreset::Fast)
        .with_camera_name("Test Camera")
        .with_gpu(1);

    assert_eq!(config.preset, Preset::P720_60);
    assert_eq!(config.codec, Codec::Hevc);
    assert_eq!(config.bitrate, 5000);
    assert_eq!(config.encoder_preset, EncoderPreset::Fast);
    assert_eq!(config.camera_name, "Test Camera");
    assert_eq!(config.gpu, 1);
}

#[test]
fn test_capture_config_effective_bitrate() {
    // Auto bitrate (0) should use preset suggestion
    let config = CaptureConfig::monitor("test").with_preset(Preset::P1080_60);
    assert_eq!(
        config.effective_bitrate(),
        Preset::P1080_60.suggested_bitrate()
    );

    // Explicit bitrate should be used
    let config = CaptureConfig::monitor("test").with_bitrate(10000);
    assert_eq!(config.effective_bitrate(), 10000);
}

#[test]
fn test_capture_config_validation() {
    // Valid config should pass
    let config = CaptureConfig::monitor("test");
    assert!(config.validate_strict().is_ok());

    // Custom preset with zero dimensions should fail
    let mut config = CaptureConfig::monitor("test");
    config.preset = Preset::Custom {
        width: 0,
        height: 1080,
        fps: 60,
    };
    assert!(config.validate_strict().is_err());

    // Custom preset with zero fps should fail
    let mut config = CaptureConfig::monitor("test");
    config.preset = Preset::Custom {
        width: 1920,
        height: 1080,
        fps: 0,
    };
    assert!(config.validate_strict().is_err());
}

#[test]
fn test_capture_config_warnings() {
    // Normal config should have no warnings
    let config = CaptureConfig::monitor("test").with_preset(Preset::P1080_60);
    let warnings = config.validate();
    assert!(warnings.is_empty());

    // Very low bitrate should warn
    let config = CaptureConfig::monitor("test")
        .with_preset(Preset::P1080_60)
        .with_bitrate(100);
    let warnings = config.validate();
    assert!(!warnings.is_empty());

    // 120fps should warn about Discord limit
    let config = CaptureConfig::monitor("test").with_preset(Preset::P1440_120);
    let warnings = config.validate();
    assert!(warnings.iter().any(|w| w.contains("Discord")));
}

#[test]
fn test_config_file_default() {
    let config = ConfigFile::default();
    assert_eq!(config.defaults.preset, "1080p60");
    assert_eq!(config.defaults.codec, "h264");
    assert_eq!(config.encoder.quality, "medium");
    assert_eq!(config.camera.name, "Nitrogen Camera");
}

#[test]
fn test_config_file_sample_parses() {
    let sample = sample_config();
    let config: ConfigFile = toml::from_str(&sample).expect("Sample config should parse");
    assert_eq!(config.defaults.preset, "1080p60");
}

#[test]
fn test_config_file_save_load() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.toml");

    // Create and save config
    let mut config = ConfigFile::default();
    config.defaults.preset = "720p60".to_string();
    config.camera.name = "Test Camera".to_string();
    config
        .save_to(config_path.clone())
        .expect("Failed to save config");

    // Load and verify
    let loaded = ConfigFile::load_from(config_path).expect("Failed to load config");
    assert_eq!(loaded.defaults.preset, "720p60");
    assert_eq!(loaded.camera.name, "Test Camera");
}

#[test]
fn test_config_file_load_nonexistent() {
    let result = ConfigFile::load_from("/nonexistent/path/config.toml".into());
    // Should return default config, not error
    assert!(result.is_ok());
}

#[test]
fn test_preset_display() {
    assert_eq!(format!("{}", Preset::P720_30), "720p30");
    assert_eq!(format!("{}", Preset::P1080_60), "1080p60");
    assert_eq!(format!("{}", Preset::P4k60), "4K60");
    assert_eq!(
        format!(
            "{}",
            Preset::Custom {
                width: 2560,
                height: 1440,
                fps: 100
            }
        ),
        "2560x1440@100fps"
    );
}

#[test]
fn test_codec_display() {
    assert_eq!(format!("{}", Codec::H264), "H.264");
    assert_eq!(format!("{}", Codec::Hevc), "HEVC");
    assert_eq!(format!("{}", Codec::Av1), "AV1");
}
