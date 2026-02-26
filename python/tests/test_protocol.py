"""Unit tests for protocol parsing and serialization.

These run without a device — pure protocol logic only.
"""

import struct
import pytest
from qcicada.protocol import (
    CMD_GET_STATUS, CMD_START, CMD_STOP, CMD_GET_CONFIG, CMD_SET_CONFIG,
    CMD_GET_STATISTICS, CMD_RESET, CMD_GET_INFO,
    RESP_ACK, RESP_NACK, RESP_CONFIG, RESP_STATISTICS, RESP_INFO,
    SUCCESS_RESPONSE, PAYLOAD_SIZE,
    build_cmd, build_start_one_shot,
    parse_status, parse_info, parse_config, parse_statistics,
    serialize_config, checksum8,
)
from qcicada.types import PostProcess, DeviceConfig


# -- Command builders --

class TestBuildCmd:
    def test_no_payload(self):
        assert build_cmd(CMD_GET_STATUS) == b'\x01'
        assert build_cmd(CMD_STOP) == b'\x05'
        assert build_cmd(CMD_GET_INFO) == b'\x0B'

    def test_with_payload(self):
        result = build_cmd(CMD_SET_CONFIG, b'\xAA\xBB')
        assert result == b'\x08\xAA\xBB'

    def test_none_payload_same_as_no_payload(self):
        assert build_cmd(CMD_GET_STATUS, None) == build_cmd(CMD_GET_STATUS)


class TestBuildStartOneShot:
    def test_format(self):
        frame = build_start_one_shot(32)
        assert frame[0:1] == CMD_START
        assert frame[1] == 0x01  # one-shot mode
        assert struct.unpack('<H', frame[2:4])[0] == 32

    def test_large_length(self):
        frame = build_start_one_shot(4096)
        assert struct.unpack('<H', frame[2:4])[0] == 4096

    def test_max_length(self):
        frame = build_start_one_shot(65535)
        assert struct.unpack('<H', frame[2:4])[0] == 65535


# -- Response mappings --

class TestResponseMappings:
    def test_command_response_mapping(self):
        assert SUCCESS_RESPONSE[CMD_GET_STATUS] == RESP_ACK
        assert SUCCESS_RESPONSE[CMD_START] == RESP_ACK
        assert SUCCESS_RESPONSE[CMD_STOP] == RESP_ACK
        assert SUCCESS_RESPONSE[CMD_GET_CONFIG] == RESP_CONFIG
        assert SUCCESS_RESPONSE[CMD_SET_CONFIG] == RESP_ACK
        assert SUCCESS_RESPONSE[CMD_GET_STATISTICS] == RESP_STATISTICS
        assert SUCCESS_RESPONSE[CMD_RESET] == RESP_ACK
        assert SUCCESS_RESPONSE[CMD_GET_INFO] == RESP_INFO

    def test_payload_sizes(self):
        assert PAYLOAD_SIZE[RESP_ACK] == 5
        assert PAYLOAD_SIZE[RESP_NACK] == 0
        assert PAYLOAD_SIZE[RESP_CONFIG] == 12
        assert PAYLOAD_SIZE[RESP_STATISTICS] == 30
        assert PAYLOAD_SIZE[RESP_INFO] == 56


# -- Command/response codes match C header --

class TestProtocolConstants:
    def test_command_codes(self):
        assert CMD_GET_STATUS == b'\x01'
        assert CMD_START == b'\x04'
        assert CMD_STOP == b'\x05'
        assert CMD_GET_CONFIG == b'\x07'
        assert CMD_SET_CONFIG == b'\x08'
        assert CMD_GET_STATISTICS == b'\x09'
        assert CMD_RESET == b'\x0A'
        assert CMD_GET_INFO == b'\x0B'

    def test_response_codes(self):
        assert RESP_ACK == b'\x11'
        assert RESP_NACK == b'\x12'
        assert RESP_CONFIG == b'\x17'
        assert RESP_STATISTICS == b'\x19'
        assert RESP_INFO == b'\x1B'


# -- Status parsing --

class TestParseStatus:
    def test_all_clear(self):
        data = struct.pack('<BI', 0x01, 13376)
        s = parse_status(data)
        assert s.initialized is True
        assert s.startup_test_in_progress is False
        assert s.voltage_low is False
        assert s.voltage_high is False
        assert s.voltage_undefined is False
        assert s.bitcount is False
        assert s.repetition_count is False
        assert s.adaptive_proportion is False
        assert s.ready_bytes == 13376

    def test_all_flags_set(self):
        data = struct.pack('<BI', 0xFF, 0)
        s = parse_status(data)
        assert s.initialized is True
        assert s.startup_test_in_progress is True
        assert s.voltage_low is True
        assert s.voltage_high is True
        assert s.voltage_undefined is True
        assert s.bitcount is True
        assert s.repetition_count is True
        assert s.adaptive_proportion is True

    def test_individual_flags(self):
        # Only voltage_low (bit 2)
        data = struct.pack('<BI', 0x04, 0)
        s = parse_status(data)
        assert s.initialized is False
        assert s.voltage_low is True
        assert s.voltage_high is False

    def test_ready_bytes_large(self):
        data = struct.pack('<BI', 0x01, 100000)
        s = parse_status(data)
        assert s.ready_bytes == 100000


# -- Info parsing --

def _make_info(core, fw, serial, hw):
    serial_buf = serial.encode().ljust(24, b'\x00')
    hw_buf = hw.encode().ljust(24, b'\x00')
    return struct.pack('<II', core, fw) + serial_buf + hw_buf


class TestParseInfo:
    def test_normal(self):
        data = _make_info(0x1000C, 0x5000E, 'QC0000000217', 'CICADA-QRNG-1.1')
        info = parse_info(data)
        assert info.core_version == 0x1000C
        assert info.fw_version == 0x5000E
        assert info.serial == 'QC0000000217'
        assert info.hw_info == 'CICADA-QRNG-1.1'

    def test_full_length_strings_no_null(self):
        serial = 'A' * 24
        hw = 'B' * 24
        data = struct.pack('<II', 1, 2) + serial.encode() + hw.encode()
        info = parse_info(data)
        assert info.serial == serial
        assert info.hw_info == hw

    def test_short_strings(self):
        data = _make_info(1, 2, 'QC1', 'HW')
        info = parse_info(data)
        assert info.serial == 'QC1'
        assert info.hw_info == 'HW'


# -- Config parsing and serialization --

def _make_config(pp, level, flags, n_lsb, hash_in, blk, ac):
    return struct.pack('<BfBBBHH', pp, level, flags, n_lsb, hash_in, blk, ac)


class TestParseConfig:
    def test_sha256_defaults(self):
        data = _make_config(0, 0.5, 0b00001111, 4, 64, 448, 2048)
        cfg = parse_config(data)
        assert cfg.postprocess == PostProcess.SHA256
        assert abs(cfg.initial_level - 0.5) < 1e-6
        assert cfg.startup_test is True
        assert cfg.auto_calibration is True
        assert cfg.repetition_count is True
        assert cfg.adaptive_proportion is True
        assert cfg.bit_count is False
        assert cfg.generate_on_error is False
        assert cfg.n_lsbits == 4
        assert cfg.hash_input_size == 64
        assert cfg.block_size == 448
        assert cfg.autocalibration_target == 2048

    def test_raw_noise(self):
        data = _make_config(1, 1.0, 0, 8, 32, 256, 1024)
        cfg = parse_config(data)
        assert cfg.postprocess == PostProcess.RAW_NOISE
        assert cfg.startup_test is False

    def test_raw_samples(self):
        data = _make_config(2, 0.0, 0, 0, 0, 0, 0)
        cfg = parse_config(data)
        assert cfg.postprocess == PostProcess.RAW_SAMPLES


class TestSerializeConfig:
    def test_roundtrip(self):
        original = DeviceConfig(
            postprocess=PostProcess.RAW_SAMPLES,
            initial_level=0.75,
            startup_test=True,
            auto_calibration=False,
            repetition_count=True,
            adaptive_proportion=True,
            bit_count=False,
            generate_on_error=True,
            n_lsbits=6,
            hash_input_size=128,
            block_size=512,
            autocalibration_target=3000,
        )
        serialized = serialize_config(original)
        assert len(serialized) == 12
        parsed = parse_config(serialized)
        assert parsed.postprocess == original.postprocess
        assert abs(parsed.initial_level - original.initial_level) < 1e-6
        assert parsed.startup_test == original.startup_test
        assert parsed.auto_calibration == original.auto_calibration
        assert parsed.repetition_count == original.repetition_count
        assert parsed.adaptive_proportion == original.adaptive_proportion
        assert parsed.bit_count == original.bit_count
        assert parsed.generate_on_error == original.generate_on_error
        assert parsed.n_lsbits == original.n_lsbits
        assert parsed.hash_input_size == original.hash_input_size
        assert parsed.block_size == original.block_size
        assert parsed.autocalibration_target == original.autocalibration_target

    def test_all_flags_on(self):
        cfg = DeviceConfig(
            postprocess=PostProcess.SHA256, initial_level=0.0,
            startup_test=True, auto_calibration=True, repetition_count=True,
            adaptive_proportion=True, bit_count=True, generate_on_error=True,
            n_lsbits=0, hash_input_size=0, block_size=0, autocalibration_target=0,
        )
        data = serialize_config(cfg)
        assert data[5] == 0b00111111

    def test_all_flags_off(self):
        cfg = DeviceConfig(
            postprocess=PostProcess.SHA256, initial_level=0.0,
            startup_test=False, auto_calibration=False, repetition_count=False,
            adaptive_proportion=False, bit_count=False, generate_on_error=False,
            n_lsbits=0, hash_input_size=0, block_size=0, autocalibration_target=0,
        )
        data = serialize_config(cfg)
        assert data[5] == 0x00


# -- Statistics parsing --

def _make_stats(gen, rep, adp, bit, spd, sens, led):
    return struct.pack('<QIIIIHf', gen, rep, adp, bit, spd, sens, led)


class TestParseStatistics:
    def test_normal(self):
        data = _make_stats(4928, 0, 1, 2, 100696, 512, 45.5)
        stats = parse_statistics(data)
        assert stats.generated_bytes == 4928
        assert stats.repetition_count_failures == 0
        assert stats.adaptive_proportion_failures == 1
        assert stats.bitcount_failures == 2
        assert stats.speed == 100696
        assert stats.sensif_average == 512
        assert abs(stats.ledctrl_level - 45.5) < 1e-6

    def test_zeros(self):
        data = _make_stats(0, 0, 0, 0, 0, 0, 0.0)
        stats = parse_statistics(data)
        assert stats.generated_bytes == 0
        assert stats.speed == 0

    def test_large_values(self):
        data = _make_stats(2**63 - 1, 2**32 - 1, 0, 0, 0, 0, 0.0)
        stats = parse_statistics(data)
        assert stats.generated_bytes == 2**63 - 1
        assert stats.repetition_count_failures == 2**32 - 1


# -- Checksum --

class TestChecksum8:
    def test_empty(self):
        assert checksum8(b'') == 0xFF

    def test_single(self):
        assert checksum8(b'\x01') == 0xFE

    def test_sum_to_ff(self):
        assert checksum8(b'\x80\x7F') == 0x00

    def test_wrapping(self):
        assert checksum8(b'\xFF\x01') == 0xFF


# -- PostProcess enum --

class TestPostProcess:
    def test_values(self):
        assert PostProcess.SHA256 == 0
        assert PostProcess.RAW_NOISE == 1
        assert PostProcess.RAW_SAMPLES == 2

    def test_from_int(self):
        assert PostProcess(0) == PostProcess.SHA256
        assert PostProcess(1) == PostProcess.RAW_NOISE
        assert PostProcess(2) == PostProcess.RAW_SAMPLES

    def test_invalid(self):
        with pytest.raises(ValueError):
            PostProcess(99)


# -- Cross-language consistency --

class TestCrossLanguageConsistency:
    """Verify Python wire format matches what Rust produces.

    These test vectors can be checked against `cargo test` output.
    """

    def test_start_one_shot_32_wire_format(self):
        frame = build_start_one_shot(32)
        assert frame == b'\x04\x01\x20\x00'

    def test_config_serialization_deterministic(self):
        cfg = DeviceConfig(
            postprocess=PostProcess.SHA256, initial_level=1.0,
            startup_test=True, auto_calibration=True, repetition_count=False,
            adaptive_proportion=False, bit_count=False, generate_on_error=False,
            n_lsbits=4, hash_input_size=64, block_size=448, autocalibration_target=2048,
        )
        data = serialize_config(cfg)
        # postprocess=0, level=1.0f LE, flags=0x03, n_lsb=4, hash=64, blk=448, ac=2048
        assert data[0] == 0  # SHA256
        assert struct.unpack('<f', data[1:5])[0] == 1.0
        assert data[5] == 0x03  # startup_test | auto_calibration
        assert data[6] == 4
        assert data[7] == 64
        assert struct.unpack('<H', data[8:10])[0] == 448
        assert struct.unpack('<H', data[10:12])[0] == 2048
