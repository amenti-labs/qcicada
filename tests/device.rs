//! Integration tests — require a physical QCicada device.
//!
//! Run with: cargo test --test device -- --test-threads=1
//!
//! These tests MUST run single-threaded (--test-threads=1) because they share
//! a single serial port. Tests are skipped if no device is detected.
//!
//! Set QCICADA_PORT to override auto-discovery:
//!   QCICADA_PORT=/dev/cu.usbserial-DK0HFP4T cargo test --test device -- --test-threads=1

use qcicada::*;
use std::io::Read;

fn open_device() -> Option<QCicada> {
    let port = std::env::var("QCICADA_PORT").ok();
    match QCicada::open(port.as_deref(), None) {
        Ok(dev) => Some(dev),
        Err(_) => {
            eprintln!("No QCicada device found — skipping integration tests");
            None
        }
    }
}

macro_rules! require_device {
    () => {
        match open_device() {
            Some(dev) => dev,
            None => return,
        }
    };
}

#[test]
fn get_info() {
    let mut qrng = require_device!();
    let info = qrng.get_info().expect("get_info failed");
    assert!(!info.serial.is_empty(), "serial should not be empty");
    assert!(!info.hw_info.is_empty(), "hw_info should not be empty");
    assert!(info.fw_version > 0, "fw_version should be nonzero");
    assert!(info.core_version > 0, "core_version should be nonzero");
    println!("Serial: {}, FW: {:#x}, HW: {}", info.serial, info.fw_version, info.hw_info);
}

#[test]
fn get_status() {
    let mut qrng = require_device!();
    let status = qrng.get_status().expect("get_status failed");
    assert!(status.initialized, "device should be initialized");
    assert!(!status.startup_test_in_progress, "startup test should be done");
    assert!(!status.voltage_low, "voltage should not be low");
    assert!(!status.voltage_high, "voltage should not be high");
    assert!(status.ready_bytes > 0, "should have bytes ready");
    println!("Ready bytes: {}", status.ready_bytes);
}

#[test]
fn get_config() {
    let mut qrng = require_device!();
    let config = qrng.get_config().expect("get_config failed");
    // Default should be SHA256
    println!("Postprocess: {:?}, block_size: {}", config.postprocess, config.block_size);
    assert!(config.block_size > 0, "block_size should be nonzero");
}

#[test]
fn get_statistics() {
    let mut qrng = require_device!();
    let stats = qrng.get_statistics().expect("get_statistics failed");
    assert!(stats.speed > 0, "speed should be nonzero");
    println!("Speed: {} bits/s, generated: {} bytes", stats.speed, stats.generated_bytes);
}

#[test]
fn random_sha256_32_bytes() {
    let mut qrng = require_device!();
    qrng.set_postprocess(PostProcess::Sha256).expect("set SHA256 failed");
    let data = qrng.random(32).expect("random failed");
    assert_eq!(data.len(), 32);
    // Extremely unlikely to be all zeros from a working QRNG
    assert!(data.iter().any(|&b| b != 0), "data should not be all zeros");
}

#[test]
fn random_sha256_different_each_time() {
    let mut qrng = require_device!();
    qrng.set_postprocess(PostProcess::Sha256).expect("set SHA256 failed");
    let a = qrng.random(32).expect("random 1 failed");
    let b = qrng.random(32).expect("random 2 failed");
    assert_ne!(a, b, "two reads should produce different data");
}

#[test]
fn random_raw_noise() {
    let mut qrng = require_device!();
    qrng.set_postprocess(PostProcess::RawNoise).expect("set RawNoise failed");
    let data = qrng.random(32).expect("random failed");
    assert_eq!(data.len(), 32);
    assert!(data.iter().any(|&b| b != 0));
    // Restore default
    qrng.set_postprocess(PostProcess::Sha256).expect("restore SHA256 failed");
}

#[test]
fn random_raw_samples() {
    let mut qrng = require_device!();
    qrng.set_postprocess(PostProcess::RawSamples).expect("set RawSamples failed");
    let data = qrng.random(32).expect("random failed");
    assert_eq!(data.len(), 32);
    // Restore default
    qrng.set_postprocess(PostProcess::Sha256).expect("restore SHA256 failed");
}

#[test]
fn random_various_sizes() {
    let mut qrng = require_device!();
    for &size in &[1, 16, 32, 64, 128, 256, 512, 1024] {
        let data = qrng.random(size).expect(&format!("random({size}) failed"));
        assert_eq!(data.len(), size as usize, "wrong length for size {size}");
    }
}

#[test]
fn random_zero_returns_empty() {
    let mut qrng = require_device!();
    let data = qrng.random(0).expect("random(0) failed");
    assert!(data.is_empty());
}

#[test]
fn fill_bytes_large() {
    let mut qrng = require_device!();
    let mut buf = vec![0u8; 256];
    qrng.fill_bytes(&mut buf).expect("fill_bytes failed");
    assert!(buf.iter().any(|&b| b != 0), "buffer should not be all zeros");
}

#[test]
fn io_read_trait() {
    let mut qrng = require_device!();
    let mut buf = [0u8; 32];
    let n = qrng.read(&mut buf).expect("io::Read failed");
    assert_eq!(n, 32);
    assert!(buf.iter().any(|&b| b != 0));
}

#[test]
fn config_roundtrip() {
    let mut qrng = require_device!();
    // Read current config
    let original = qrng.get_config().expect("get_config failed");

    // Modify and write
    let mut modified = original.clone();
    modified.postprocess = PostProcess::RawNoise;
    qrng.set_config(&modified).expect("set_config failed");

    // Verify
    let readback = qrng.get_config().expect("get_config readback failed");
    assert_eq!(readback.postprocess, PostProcess::RawNoise);

    // Restore original
    qrng.set_config(&original).expect("restore config failed");
    let restored = qrng.get_config().expect("get_config restore check failed");
    assert_eq!(restored.postprocess, original.postprocess);
}

#[test]
fn set_postprocess_convenience() {
    let mut qrng = require_device!();
    let original = qrng.get_config().expect("get_config failed");

    qrng.set_postprocess(PostProcess::RawNoise).expect("set RawNoise");
    let cfg = qrng.get_config().expect("readback");
    assert_eq!(cfg.postprocess, PostProcess::RawNoise);
    // Other fields should be preserved
    assert_eq!(cfg.block_size, original.block_size);
    assert_eq!(cfg.auto_calibration, original.auto_calibration);

    // Restore
    qrng.set_postprocess(original.postprocess).expect("restore");
}

#[test]
fn stop_is_safe() {
    let mut qrng = require_device!();
    // stop() should not fail even when nothing is running
    qrng.stop().expect("stop failed");
    // Device should still work after stop
    let _ = qrng.get_status().expect("get_status after stop failed");
}

#[test]
fn discover_devices_finds_device() {
    let devices = discover_devices();
    if devices.is_empty() {
        eprintln!("No device found — skipping");
        return;
    }
    let dev = &devices[0];
    assert!(!dev.info.serial.is_empty());
    assert!(!dev.port.is_empty());
    println!("Discovered: {} at {}", dev.info.serial, dev.port);
}

#[test]
fn probe_device_on_bogus_port() {
    let info = probe_device("/dev/nonexistent_port_xyz");
    assert!(info.is_none(), "probe should return None for bogus port");
}

#[test]
fn signed_read_32_bytes() {
    let mut qrng = require_device!();
    qrng.set_postprocess(PostProcess::Sha256).expect("set SHA256 failed");
    let result = qrng.signed_read(32).expect("signed_read failed");
    assert_eq!(result.data.len(), 32);
    assert_eq!(result.signature.len(), 64);
    assert!(result.data.iter().any(|&b| b != 0), "data should not be all zeros");
    assert!(result.signature.iter().any(|&b| b != 0), "signature should not be all zeros");
}

#[test]
fn signed_read_different_each_time() {
    let mut qrng = require_device!();
    let a = qrng.signed_read(32).expect("signed_read 1 failed");
    let b = qrng.signed_read(32).expect("signed_read 2 failed");
    assert_ne!(a.data, b.data, "two signed reads should produce different data");
    // Signatures should also differ (different data → different sig)
    assert_ne!(a.signature, b.signature, "signatures should differ");
}

#[test]
fn continuous_mode_read() {
    let mut qrng = require_device!();
    qrng.start_continuous().expect("start_continuous failed");
    let data = qrng.read_continuous(64).expect("read_continuous failed");
    assert_eq!(data.len(), 64);
    assert!(data.iter().any(|&b| b != 0), "continuous data should not be all zeros");
    qrng.stop().expect("stop after continuous failed");
    // Device should still work after stopping continuous mode
    let _ = qrng.get_status().expect("get_status after continuous stop failed");
}

#[test]
fn continuous_mode_multiple_reads() {
    let mut qrng = require_device!();
    qrng.start_continuous().expect("start_continuous failed");
    let a = qrng.read_continuous(32).expect("read 1 failed");
    let b = qrng.read_continuous(32).expect("read 2 failed");
    assert_ne!(a, b, "consecutive reads should differ");
    qrng.stop().expect("stop failed");
}

#[test]
fn get_dev_pub_key() {
    let mut qrng = require_device!();
    let pub_key = qrng.get_dev_pub_key().expect("get_dev_pub_key failed");
    assert_eq!(pub_key.len(), 64, "public key should be 64 bytes");
    assert!(pub_key.iter().any(|&b| b != 0), "public key should not be all zeros");
    println!("Device pub key: {}", hex::encode(&pub_key));
}

#[test]
fn get_dev_certificate() {
    let mut qrng = require_device!();
    let cert = qrng.get_dev_certificate().expect("get_dev_certificate failed");
    assert_eq!(cert.len(), 64, "certificate should be 64 bytes");
    println!("Device certificate: {}", hex::encode(&cert));
}

#[test]
fn signed_read_verified_with_device_key() {
    let mut qrng = require_device!();
    let pub_key = qrng.get_dev_pub_key().expect("get_dev_pub_key failed");

    // Verified signed read should succeed
    let result = qrng
        .signed_read_verified(32, &pub_key)
        .expect("signed_read_verified failed");
    assert_eq!(result.data.len(), 32);
    assert_eq!(result.signature.len(), 64);
    println!("Verified signed read: {} bytes OK", result.data.len());
}

#[test]
fn signed_read_verified_wrong_key_fails() {
    let mut qrng = require_device!();
    // Use a bogus public key — verification should fail
    let bogus_key = vec![0x42u8; 64];
    let result = qrng.signed_read_verified(32, &bogus_key);
    assert!(result.is_err(), "verification with wrong key should fail");
}

// NOTE: probe_device_on_known_port and open_by_serial are not tested here because
// integration tests run in parallel threads and the serial port can only be opened
// by one test at a time. These are exercised by discover_devices_finds_device above
// (which internally calls probe_device).
