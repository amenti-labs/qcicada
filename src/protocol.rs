//! QCC wire protocol: command builders, response parsers, constants.
//!
//! Pure functions with no I/O — reusable with any transport.

use crate::types::*;
use crate::QCicadaError;

// --- Command codes ---
pub const CMD_GET_STATUS: u8 = 0x01;
pub const CMD_START: u8 = 0x04;
pub const CMD_STOP: u8 = 0x05;
pub const CMD_GET_CONFIG: u8 = 0x07;
pub const CMD_SET_CONFIG: u8 = 0x08;
pub const CMD_GET_STATISTICS: u8 = 0x09;
pub const CMD_RESET: u8 = 0x0A;
pub const CMD_GET_INFO: u8 = 0x0B;

// --- Response codes ---
pub const RESP_ACK: u8 = 0x11;
pub const RESP_NACK: u8 = 0x12;
pub const RESP_CONFIG: u8 = 0x17;
pub const RESP_STATISTICS: u8 = 0x19;
pub const RESP_INFO: u8 = 0x1B;

// --- Payload sizes ---
pub const PAYLOAD_ACK: usize = 5;
pub const PAYLOAD_CONFIG: usize = 12;
pub const PAYLOAD_STATISTICS: usize = 30;
pub const PAYLOAD_INFO: usize = 56;

// --- Start mode ---
pub const START_ONE_SHOT: u8 = 0x01;

pub const MAX_BLOCK_SIZE: usize = 4096;

/// Returns the expected success response code for a command.
pub fn expected_response(cmd: u8) -> Option<u8> {
    match cmd {
        CMD_GET_STATUS => Some(RESP_ACK),
        CMD_START => Some(RESP_ACK),
        CMD_STOP => Some(RESP_ACK),
        CMD_GET_CONFIG => Some(RESP_CONFIG),
        CMD_SET_CONFIG => Some(RESP_ACK),
        CMD_GET_STATISTICS => Some(RESP_STATISTICS),
        CMD_RESET => Some(RESP_ACK),
        CMD_GET_INFO => Some(RESP_INFO),
        _ => None,
    }
}

/// Returns the payload size for a response code.
pub fn payload_size(resp: u8) -> usize {
    match resp {
        RESP_ACK => PAYLOAD_ACK,
        RESP_NACK => 0,
        RESP_CONFIG => PAYLOAD_CONFIG,
        RESP_STATISTICS => PAYLOAD_STATISTICS,
        RESP_INFO => PAYLOAD_INFO,
        _ => 0,
    }
}

/// Build a command frame: command byte + optional payload.
pub fn build_cmd(code: u8, payload: Option<&[u8]>) -> Vec<u8> {
    let mut frame = vec![code];
    if let Some(p) = payload {
        frame.extend_from_slice(p);
    }
    frame
}

/// Build a START command for one-shot mode.
pub fn build_start_one_shot(length: u16) -> Vec<u8> {
    let mut frame = vec![CMD_START, START_ONE_SHOT];
    frame.extend_from_slice(&length.to_le_bytes());
    frame
}

/// Parse a 5-byte ACK/status payload.
pub fn parse_status(data: &[u8]) -> Result<DeviceStatus, QCicadaError> {
    if data.len() < PAYLOAD_ACK {
        return Err(QCicadaError::Protocol("Status payload too short".into()));
    }
    let flags = data[0];
    let ready = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    Ok(DeviceStatus {
        initialized: flags & 1 != 0,
        startup_test_in_progress: (flags >> 1) & 1 != 0,
        voltage_low: (flags >> 2) & 1 != 0,
        voltage_high: (flags >> 3) & 1 != 0,
        voltage_undefined: (flags >> 4) & 1 != 0,
        bitcount: (flags >> 5) & 1 != 0,
        repetition_count: (flags >> 6) & 1 != 0,
        adaptive_proportion: (flags >> 7) & 1 != 0,
        ready_bytes: ready,
    })
}

/// Parse a 56-byte INFO response payload.
pub fn parse_info(data: &[u8]) -> Result<DeviceInfo, QCicadaError> {
    if data.len() < PAYLOAD_INFO {
        return Err(QCicadaError::Protocol("Info payload too short".into()));
    }
    let core_version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let fw_version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

    let serial_raw = &data[8..32];
    let serial_end = serial_raw.iter().position(|&b| b == 0).unwrap_or(24);
    let serial = String::from_utf8_lossy(&serial_raw[..serial_end]).into_owned();

    let hw_raw = &data[32..56];
    let hw_end = hw_raw.iter().position(|&b| b == 0).unwrap_or(24);
    let hw_info = String::from_utf8_lossy(&hw_raw[..hw_end]).into_owned();

    Ok(DeviceInfo {
        core_version,
        fw_version,
        serial,
        hw_info,
    })
}

/// Parse a 12-byte CONFIG response payload.
pub fn parse_config(data: &[u8]) -> Result<DeviceConfig, QCicadaError> {
    if data.len() < PAYLOAD_CONFIG {
        return Err(QCicadaError::Protocol("Config payload too short".into()));
    }
    let pp = PostProcess::try_from(data[0])
        .map_err(|v| QCicadaError::Protocol(format!("Unknown postprocess mode: {v}")))?;
    let level = f32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let flags = data[5];

    Ok(DeviceConfig {
        postprocess: pp,
        initial_level: level,
        startup_test: flags & 1 != 0,
        auto_calibration: (flags >> 1) & 1 != 0,
        repetition_count: (flags >> 2) & 1 != 0,
        adaptive_proportion: (flags >> 3) & 1 != 0,
        bit_count: (flags >> 4) & 1 != 0,
        generate_on_error: (flags >> 5) & 1 != 0,
        n_lsbits: data[6],
        hash_input_size: data[7],
        block_size: u16::from_le_bytes([data[8], data[9]]),
        autocalibration_target: u16::from_le_bytes([data[10], data[11]]),
    })
}

/// Serialize a DeviceConfig to 12 bytes for SET_CONFIG.
pub fn serialize_config(config: &DeviceConfig) -> Vec<u8> {
    let flags: u8 = (config.startup_test as u8)
        | ((config.auto_calibration as u8) << 1)
        | ((config.repetition_count as u8) << 2)
        | ((config.adaptive_proportion as u8) << 3)
        | ((config.bit_count as u8) << 4)
        | ((config.generate_on_error as u8) << 5);

    let mut buf = Vec::with_capacity(PAYLOAD_CONFIG);
    buf.push(config.postprocess as u8);
    buf.extend_from_slice(&config.initial_level.to_le_bytes());
    buf.push(flags);
    buf.push(config.n_lsbits);
    buf.push(config.hash_input_size);
    buf.extend_from_slice(&config.block_size.to_le_bytes());
    buf.extend_from_slice(&config.autocalibration_target.to_le_bytes());
    buf
}

/// Parse a 30-byte STATISTICS response payload.
pub fn parse_statistics(data: &[u8]) -> Result<DeviceStatistics, QCicadaError> {
    if data.len() < PAYLOAD_STATISTICS {
        return Err(QCicadaError::Protocol("Statistics payload too short".into()));
    }
    let generated_bytes = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let rep_failures = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let adp_failures = u32::from_le_bytes(data[12..16].try_into().unwrap());
    let bit_failures = u32::from_le_bytes(data[16..20].try_into().unwrap());
    let speed = u32::from_le_bytes(data[20..24].try_into().unwrap());
    let sensif = u16::from_le_bytes(data[24..26].try_into().unwrap());
    let ledctrl = f32::from_le_bytes(data[26..30].try_into().unwrap());

    Ok(DeviceStatistics {
        generated_bytes,
        repetition_count_failures: rep_failures,
        adaptive_proportion_failures: adp_failures,
        bitcount_failures: bit_failures,
        speed,
        sensif_average: sensif,
        ledctrl_level: ledctrl,
    })
}

/// Ones-complement 8-bit checksum (for firmware update chunks).
pub fn checksum8(data: &[u8]) -> u8 {
    let sum: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    !sum
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Command builder tests --

    #[test]
    fn build_cmd_no_payload() {
        assert_eq!(build_cmd(CMD_GET_STATUS, None), vec![0x01]);
        assert_eq!(build_cmd(CMD_STOP, None), vec![0x05]);
        assert_eq!(build_cmd(CMD_GET_INFO, None), vec![0x0B]);
    }

    #[test]
    fn build_cmd_with_payload() {
        let frame = build_cmd(CMD_SET_CONFIG, Some(&[0xAA, 0xBB]));
        assert_eq!(frame, vec![0x08, 0xAA, 0xBB]);
    }

    #[test]
    fn build_start_one_shot_format() {
        let frame = build_start_one_shot(32);
        assert_eq!(frame[0], CMD_START);        // command byte
        assert_eq!(frame[1], START_ONE_SHOT);   // mode = one-shot
        assert_eq!(frame[2], 32);               // length low byte
        assert_eq!(frame[3], 0);                // length high byte
        assert_eq!(frame.len(), 4);
    }

    #[test]
    fn build_start_one_shot_large() {
        let frame = build_start_one_shot(4096);
        assert_eq!(u16::from_le_bytes([frame[2], frame[3]]), 4096);
    }

    #[test]
    fn build_start_one_shot_max() {
        let frame = build_start_one_shot(u16::MAX);
        assert_eq!(u16::from_le_bytes([frame[2], frame[3]]), 65535);
    }

    // -- Response mapping tests --

    #[test]
    fn expected_response_mapping() {
        assert_eq!(expected_response(CMD_GET_STATUS), Some(RESP_ACK));
        assert_eq!(expected_response(CMD_START), Some(RESP_ACK));
        assert_eq!(expected_response(CMD_STOP), Some(RESP_ACK));
        assert_eq!(expected_response(CMD_GET_CONFIG), Some(RESP_CONFIG));
        assert_eq!(expected_response(CMD_SET_CONFIG), Some(RESP_ACK));
        assert_eq!(expected_response(CMD_GET_STATISTICS), Some(RESP_STATISTICS));
        assert_eq!(expected_response(CMD_RESET), Some(RESP_ACK));
        assert_eq!(expected_response(CMD_GET_INFO), Some(RESP_INFO));
        assert_eq!(expected_response(0xFF), None);
    }

    #[test]
    fn payload_size_mapping() {
        assert_eq!(payload_size(RESP_ACK), 5);
        assert_eq!(payload_size(RESP_NACK), 0);
        assert_eq!(payload_size(RESP_CONFIG), 12);
        assert_eq!(payload_size(RESP_STATISTICS), 30);
        assert_eq!(payload_size(RESP_INFO), 56);
        assert_eq!(payload_size(0xFF), 0);
    }

    // -- Status parsing tests --

    #[test]
    fn parse_status_all_clear() {
        let data = [0x01, 0x40, 0x34, 0x00, 0x00]; // initialized, 13376 ready
        let s = parse_status(&data).unwrap();
        assert!(s.initialized);
        assert!(!s.startup_test_in_progress);
        assert!(!s.voltage_low);
        assert!(!s.voltage_high);
        assert!(!s.voltage_undefined);
        assert!(!s.bitcount);
        assert!(!s.repetition_count);
        assert!(!s.adaptive_proportion);
        assert_eq!(s.ready_bytes, 13376);
    }

    #[test]
    fn parse_status_all_flags_set() {
        let data = [0xFF, 0x00, 0x00, 0x00, 0x00];
        let s = parse_status(&data).unwrap();
        assert!(s.initialized);
        assert!(s.startup_test_in_progress);
        assert!(s.voltage_low);
        assert!(s.voltage_high);
        assert!(s.voltage_undefined);
        assert!(s.bitcount);
        assert!(s.repetition_count);
        assert!(s.adaptive_proportion);
    }

    #[test]
    fn parse_status_individual_flags() {
        // Only voltage_low (bit 2) set
        let data = [0x04, 0x00, 0x00, 0x00, 0x00];
        let s = parse_status(&data).unwrap();
        assert!(!s.initialized);
        assert!(s.voltage_low);
        assert!(!s.voltage_high);
    }

    #[test]
    fn parse_status_too_short() {
        assert!(parse_status(&[0x01, 0x00, 0x00]).is_err());
    }

    // -- Info parsing tests --

    fn make_info_payload(core: u32, fw: u32, serial: &str, hw: &str) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&core.to_le_bytes());
        data.extend_from_slice(&fw.to_le_bytes());
        let mut serial_buf = [0u8; 24];
        serial_buf[..serial.len()].copy_from_slice(serial.as_bytes());
        data.extend_from_slice(&serial_buf);
        let mut hw_buf = [0u8; 24];
        hw_buf[..hw.len()].copy_from_slice(hw.as_bytes());
        data.extend_from_slice(&hw_buf);
        data
    }

    #[test]
    fn parse_info_normal() {
        let data = make_info_payload(0x1000C, 0x5000E, "QC0000000217", "CICADA-QRNG-1.1");
        let info = parse_info(&data).unwrap();
        assert_eq!(info.core_version, 0x1000C);
        assert_eq!(info.fw_version, 0x5000E);
        assert_eq!(info.serial, "QC0000000217");
        assert_eq!(info.hw_info, "CICADA-QRNG-1.1");
    }

    #[test]
    fn parse_info_full_length_strings() {
        // 24-char strings with no null terminator
        let serial = "ABCDEFGHIJKLMNOPQRSTUVWX";
        let hw = "123456789012345678901234";
        let data = make_info_payload(1, 2, serial, hw);
        let info = parse_info(&data).unwrap();
        assert_eq!(info.serial, serial);
        assert_eq!(info.hw_info, hw);
    }

    #[test]
    fn parse_info_too_short() {
        assert!(parse_info(&[0u8; 10]).is_err());
    }

    // -- Config parse/serialize roundtrip tests --

    fn make_config_payload(pp: u8, level: f32, flags: u8, n_lsb: u8, hash_in: u8, blk: u16, ac: u16) -> Vec<u8> {
        let mut data = vec![pp];
        data.extend_from_slice(&level.to_le_bytes());
        data.push(flags);
        data.push(n_lsb);
        data.push(hash_in);
        data.extend_from_slice(&blk.to_le_bytes());
        data.extend_from_slice(&ac.to_le_bytes());
        data
    }

    #[test]
    fn parse_config_sha256_defaults() {
        let data = make_config_payload(0, 0.5, 0b00001111, 4, 64, 448, 2048);
        let cfg = parse_config(&data).unwrap();
        assert_eq!(cfg.postprocess, PostProcess::Sha256);
        assert!((cfg.initial_level - 0.5).abs() < f32::EPSILON);
        assert!(cfg.startup_test);
        assert!(cfg.auto_calibration);
        assert!(cfg.repetition_count);
        assert!(cfg.adaptive_proportion);
        assert!(!cfg.bit_count);
        assert!(!cfg.generate_on_error);
        assert_eq!(cfg.n_lsbits, 4);
        assert_eq!(cfg.hash_input_size, 64);
        assert_eq!(cfg.block_size, 448);
        assert_eq!(cfg.autocalibration_target, 2048);
    }

    #[test]
    fn parse_config_raw_noise() {
        let data = make_config_payload(1, 1.0, 0, 8, 32, 256, 1024);
        let cfg = parse_config(&data).unwrap();
        assert_eq!(cfg.postprocess, PostProcess::RawNoise);
        assert!(!cfg.startup_test);
    }

    #[test]
    fn parse_config_invalid_postprocess() {
        let data = make_config_payload(99, 0.0, 0, 0, 0, 0, 0);
        assert!(parse_config(&data).is_err());
    }

    #[test]
    fn parse_config_too_short() {
        assert!(parse_config(&[0u8; 5]).is_err());
    }

    #[test]
    fn config_roundtrip() {
        let original = DeviceConfig {
            postprocess: PostProcess::RawSamples,
            initial_level: 0.75,
            startup_test: true,
            auto_calibration: false,
            repetition_count: true,
            adaptive_proportion: true,
            bit_count: false,
            generate_on_error: true,
            n_lsbits: 6,
            hash_input_size: 128,
            block_size: 512,
            autocalibration_target: 3000,
        };
        let serialized = serialize_config(&original);
        assert_eq!(serialized.len(), PAYLOAD_CONFIG);

        let parsed = parse_config(&serialized).unwrap();
        assert_eq!(parsed.postprocess, original.postprocess);
        assert!((parsed.initial_level - original.initial_level).abs() < f32::EPSILON);
        assert_eq!(parsed.startup_test, original.startup_test);
        assert_eq!(parsed.auto_calibration, original.auto_calibration);
        assert_eq!(parsed.repetition_count, original.repetition_count);
        assert_eq!(parsed.adaptive_proportion, original.adaptive_proportion);
        assert_eq!(parsed.bit_count, original.bit_count);
        assert_eq!(parsed.generate_on_error, original.generate_on_error);
        assert_eq!(parsed.n_lsbits, original.n_lsbits);
        assert_eq!(parsed.hash_input_size, original.hash_input_size);
        assert_eq!(parsed.block_size, original.block_size);
        assert_eq!(parsed.autocalibration_target, original.autocalibration_target);
    }

    #[test]
    fn config_all_flags_on() {
        let cfg = DeviceConfig {
            postprocess: PostProcess::Sha256,
            initial_level: 0.0,
            startup_test: true,
            auto_calibration: true,
            repetition_count: true,
            adaptive_proportion: true,
            bit_count: true,
            generate_on_error: true,
            n_lsbits: 0,
            hash_input_size: 0,
            block_size: 0,
            autocalibration_target: 0,
        };
        let data = serialize_config(&cfg);
        assert_eq!(data[5], 0b00111111); // all 6 flag bits set
    }

    #[test]
    fn config_all_flags_off() {
        let cfg = DeviceConfig {
            postprocess: PostProcess::Sha256,
            initial_level: 0.0,
            startup_test: false,
            auto_calibration: false,
            repetition_count: false,
            adaptive_proportion: false,
            bit_count: false,
            generate_on_error: false,
            n_lsbits: 0,
            hash_input_size: 0,
            block_size: 0,
            autocalibration_target: 0,
        };
        let data = serialize_config(&cfg);
        assert_eq!(data[5], 0x00);
    }

    // -- Statistics parsing tests --

    fn make_stats_payload(gen: u64, rep: u32, adp: u32, bit: u32, spd: u32, sens: u16, led: f32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&gen.to_le_bytes());
        data.extend_from_slice(&rep.to_le_bytes());
        data.extend_from_slice(&adp.to_le_bytes());
        data.extend_from_slice(&bit.to_le_bytes());
        data.extend_from_slice(&spd.to_le_bytes());
        data.extend_from_slice(&sens.to_le_bytes());
        data.extend_from_slice(&led.to_le_bytes());
        data
    }

    #[test]
    fn parse_statistics_normal() {
        let data = make_stats_payload(4928, 0, 1, 2, 100696, 512, 45.5);
        let stats = parse_statistics(&data).unwrap();
        assert_eq!(stats.generated_bytes, 4928);
        assert_eq!(stats.repetition_count_failures, 0);
        assert_eq!(stats.adaptive_proportion_failures, 1);
        assert_eq!(stats.bitcount_failures, 2);
        assert_eq!(stats.speed, 100696);
        assert_eq!(stats.sensif_average, 512);
        assert!((stats.ledctrl_level - 45.5).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_statistics_too_short() {
        assert!(parse_statistics(&[0u8; 20]).is_err());
    }

    // -- Checksum tests --

    #[test]
    fn checksum8_empty() {
        assert_eq!(checksum8(&[]), 0xFF);
    }

    #[test]
    fn checksum8_single() {
        assert_eq!(checksum8(&[0x01]), 0xFE);
    }

    #[test]
    fn checksum8_sum_to_ff() {
        // 0x80 + 0x7F = 0xFF -> complement = 0x00
        assert_eq!(checksum8(&[0x80, 0x7F]), 0x00);
    }

    #[test]
    fn checksum8_wrapping() {
        // 0xFF + 0x01 = 0x00 (wrapping) -> complement = 0xFF
        assert_eq!(checksum8(&[0xFF, 0x01]), 0xFF);
    }

    // -- PostProcess enum tests --

    #[test]
    fn postprocess_try_from() {
        assert_eq!(PostProcess::try_from(0), Ok(PostProcess::Sha256));
        assert_eq!(PostProcess::try_from(1), Ok(PostProcess::RawNoise));
        assert_eq!(PostProcess::try_from(2), Ok(PostProcess::RawSamples));
        assert_eq!(PostProcess::try_from(3), Err(3));
        assert_eq!(PostProcess::try_from(255), Err(255));
    }

    #[test]
    fn postprocess_to_u8() {
        assert_eq!(PostProcess::Sha256 as u8, 0);
        assert_eq!(PostProcess::RawNoise as u8, 1);
        assert_eq!(PostProcess::RawSamples as u8, 2);
    }

    // -- Cross-language consistency: verify wire format matches C header --

    #[test]
    fn command_codes_match_c_header() {
        assert_eq!(CMD_GET_STATUS, 0x01);
        assert_eq!(CMD_START, 0x04);
        assert_eq!(CMD_STOP, 0x05);
        assert_eq!(CMD_GET_CONFIG, 0x07);
        assert_eq!(CMD_SET_CONFIG, 0x08);
        assert_eq!(CMD_GET_STATISTICS, 0x09);
        assert_eq!(CMD_RESET, 0x0A);
        assert_eq!(CMD_GET_INFO, 0x0B);
    }

    #[test]
    fn response_codes_match_c_header() {
        assert_eq!(RESP_ACK, 0x11);
        assert_eq!(RESP_NACK, 0x12);
        assert_eq!(RESP_CONFIG, 0x17);
        assert_eq!(RESP_STATISTICS, 0x19);
        assert_eq!(RESP_INFO, 0x1B);
    }
}
