//! Integration tests for encoding subsystem
//!
//! Tests that require NVENC hardware are marked with #[ignore].

mod mocks;

use nitrogen_core::config::Codec;
use nitrogen_core::encode;

#[test]
fn test_codec_encoder_names() {
    assert_eq!(Codec::H264.nvenc_encoder(), "h264_nvenc");
    assert_eq!(Codec::Hevc.nvenc_encoder(), "hevc_nvenc");
    assert_eq!(Codec::Av1.nvenc_encoder(), "av1_nvenc");
}

#[test]
fn test_encoder_capability_estimates() {
    // These are estimates, so we just verify the structure
    for codec in [Codec::H264, Codec::Hevc, Codec::Av1] {
        if encode::codec_available(codec) {
            let caps = encode::get_encoder_capabilities(codec);
            assert!(
                caps.is_some(),
                "Should have capabilities for available codec"
            );

            let caps = caps.unwrap();
            assert!(caps.max_width >= 1920);
            assert!(caps.max_height >= 1080);
        }
    }
}

#[test]
fn test_gpu_info_structure() {
    // This might return None if no NVIDIA GPU, which is fine
    if let Some(gpu) = encode::get_gpu_info() {
        // If we get GPU info, it should have valid data
        assert!(!gpu.name.is_empty(), "GPU name should not be empty");
        assert!(
            !gpu.driver_version.is_empty(),
            "Driver version should not be empty"
        );
        // VRAM might be 0 in edge cases, but typically should be positive
    }
}

#[test]
fn test_available_encoders_format() {
    let encoders = encode::available_encoders();
    for encoder in encoders {
        // Each encoder name should be descriptive
        assert!(!encoder.is_empty());
        // Should contain either nvenc or the codec name
        let lower = encoder.to_lowercase();
        assert!(
            lower.contains("nvenc")
                || lower.contains("264")
                || lower.contains("265")
                || lower.contains("hevc")
                || lower.contains("av1"),
            "Encoder name should be descriptive: {}",
            encoder
        );
    }
}

// Hardware-dependent tests

#[test]
#[ignore = "Requires NVIDIA GPU"]
fn test_nvenc_detection() {
    let available = encode::nvenc_available();
    eprintln!("NVENC available: {}", available);

    if available {
        let encoders = encode::available_encoders();
        eprintln!("Available encoders: {:?}", encoders);
        assert!(!encoders.is_empty());
    }
}

#[test]
#[ignore = "Requires NVIDIA GPU with NVENC"]
fn test_h264_encoder_available() {
    if encode::nvenc_available() {
        assert!(
            encode::codec_available(Codec::H264),
            "H.264 should be available on NVENC-capable systems"
        );

        let caps = encode::get_encoder_capabilities(Codec::H264);
        assert!(caps.is_some());

        let caps = caps.unwrap();
        eprintln!("H.264 capabilities:");
        eprintln!("  B-frames: {}", caps.b_frames);
        eprintln!("  10-bit: {}", caps.bit_10);
        eprintln!("  Lookahead: {}", caps.lookahead);
        eprintln!("  Max resolution: {}x{}", caps.max_width, caps.max_height);
    }
}

#[test]
#[ignore = "Requires NVIDIA GPU with NVENC"]
fn test_hevc_encoder_available() {
    if encode::nvenc_available() && encode::codec_available(Codec::Hevc) {
        let caps = encode::get_encoder_capabilities(Codec::Hevc);
        assert!(caps.is_some());

        let caps = caps.unwrap();
        eprintln!("HEVC capabilities:");
        eprintln!("  B-frames: {}", caps.b_frames);
        eprintln!("  10-bit: {}", caps.bit_10);
    }
}

#[test]
#[ignore = "Requires RTX 40 series GPU"]
fn test_av1_encoder_available() {
    if encode::nvenc_available() && encode::codec_available(Codec::Av1) {
        let caps = encode::get_encoder_capabilities(Codec::Av1);
        assert!(caps.is_some());

        let caps = caps.unwrap();
        eprintln!("AV1 capabilities:");
        eprintln!("  B-frames: {}", caps.b_frames); // AV1 NVENC doesn't support B-frames
        eprintln!("  10-bit: {}", caps.bit_10);

        // AV1 NVENC typically doesn't support B-frames
        assert!(!caps.b_frames, "AV1 NVENC should not support B-frames");
    }
}

#[test]
fn test_test_frame_sizes() {
    // Test common Discord streaming resolutions
    let resolutions = [
        (1280, 720),  // 720p
        (1920, 1080), // 1080p
        (2560, 1440), // 1440p
        (3840, 2160), // 4K
    ];

    for (width, height) in resolutions {
        let frame = mocks::create_test_frame(width, height, [0, 0, 0, 255]);
        assert_eq!(frame.format.width, width);
        assert_eq!(frame.format.height, height);

        // Verify data size is correct
        if let nitrogen_core::types::FrameData::Memory(data) = &frame.data {
            let expected_size = (width * height * 4) as usize;
            assert_eq!(
                data.len(),
                expected_size,
                "Frame data size mismatch for {}x{}",
                width,
                height
            );
        }
    }
}

#[test]
fn test_gradient_frame_variety() {
    let frame = mocks::create_gradient_frame(256, 256);

    if let nitrogen_core::types::FrameData::Memory(data) = &frame.data {
        // Check that corners have different colors (gradient should vary)
        let top_left = &data[0..4];
        let top_right = &data[(255 * 4)..(256 * 4)];
        let bottom_left = &data[(255 * 256 * 4)..(255 * 256 * 4 + 4)];

        // At least some channels should differ between corners
        assert_ne!(top_left, top_right, "Gradient should vary horizontally");
        assert_ne!(top_left, bottom_left, "Gradient should vary vertically");
    }
}
