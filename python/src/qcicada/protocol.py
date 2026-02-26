"""QCC wire protocol: command builders, response parsers, constants.

Pure functions with no I/O — reusable with any transport.
"""

from __future__ import annotations

import struct
from .types import (
    PostProcess, DeviceInfo, DeviceStatus, DeviceConfig, DeviceStatistics,
)

# --- Command codes ---
CMD_GET_STATUS = b'\x01'
CMD_START = b'\x04'
CMD_STOP = b'\x05'
CMD_GET_CONFIG = b'\x07'
CMD_SET_CONFIG = b'\x08'
CMD_GET_STATISTICS = b'\x09'
CMD_RESET = b'\x0A'
CMD_GET_INFO = b'\x0B'
CMD_SIGNED_READ = b'\x51'

# --- Response codes ---
RESP_ACK = b'\x11'
RESP_NACK = b'\x12'
RESP_CONFIG = b'\x17'
RESP_STATISTICS = b'\x19'
RESP_INFO = b'\x1B'
RESP_SIGNED_READ = b'\x52'

# --- Expected response for each command ---
SUCCESS_RESPONSE: dict[bytes, bytes] = {
    CMD_GET_STATUS: RESP_ACK,
    CMD_START: RESP_ACK,
    CMD_STOP: RESP_ACK,
    CMD_GET_CONFIG: RESP_CONFIG,
    CMD_SET_CONFIG: RESP_ACK,
    CMD_GET_STATISTICS: RESP_STATISTICS,
    CMD_RESET: RESP_ACK,
    CMD_GET_INFO: RESP_INFO,
    CMD_SIGNED_READ: RESP_SIGNED_READ,
}

# --- Payload sizes for each response code ---
PAYLOAD_SIZE: dict[bytes, int] = {
    RESP_ACK: 5,          # 1 byte flags + 4 bytes ready_bytes
    RESP_NACK: 0,
    RESP_CONFIG: 12,      # Full-mode cmdctrl_config_t
    RESP_STATISTICS: 30,  # Full-mode cmdctrl_statistics_t
    RESP_INFO: 56,        # 4+4+24+24
    RESP_SIGNED_READ: 0,  # data + signature follow separately
}

# --- Start mode ---
START_CONTINUOUS = 0x00
START_ONE_SHOT = 0x01

# --- Signature ---
SIGNATURE_LEN = 64

MAX_BLOCK_SIZE = 4096


def build_cmd(code: bytes, payload: bytes | None = None) -> bytes:
    """Build a command frame: command byte + optional payload."""
    if payload is not None:
        return code + payload
    return code


def build_start_one_shot(length: int) -> bytes:
    """Build a START command for one-shot mode."""
    payload = struct.pack('<BH', START_ONE_SHOT, length)
    return build_cmd(CMD_START, payload)


def build_start_continuous() -> bytes:
    """Build a START command for continuous mode."""
    payload = struct.pack('<BH', START_CONTINUOUS, 0)
    return build_cmd(CMD_START, payload)


def build_signed_read(length: int) -> bytes:
    """Build a SIGNED_READ command."""
    payload = struct.pack('<H', length)
    return build_cmd(CMD_SIGNED_READ, payload)


def parse_status(data: bytes) -> DeviceStatus:
    """Parse a 5-byte ACK/status payload."""
    flags, ready = struct.unpack('<BI', data[:5])
    return DeviceStatus(
        initialized=bool(flags & 1),
        startup_test_in_progress=bool((flags >> 1) & 1),
        voltage_low=bool((flags >> 2) & 1),
        voltage_high=bool((flags >> 3) & 1),
        voltage_undefined=bool((flags >> 4) & 1),
        bitcount=bool((flags >> 5) & 1),
        repetition_count=bool((flags >> 6) & 1),
        adaptive_proportion=bool((flags >> 7) & 1),
        ready_bytes=ready,
    )


def parse_info(data: bytes) -> DeviceInfo:
    """Parse a 56-byte INFO response payload."""
    core_ver, fw_ver, serial_raw, hw_raw = struct.unpack('<II24s24s', data[:56])
    serial_end = serial_raw.index(0) if 0 in serial_raw else len(serial_raw)
    serial = serial_raw[:serial_end].decode('utf-8')
    hw_end = hw_raw.index(0) if 0 in hw_raw else len(hw_raw)
    hw_info = hw_raw[:hw_end].decode('utf-8')
    return DeviceInfo(
        core_version=core_ver,
        fw_version=fw_ver,
        serial=serial,
        hw_info=hw_info,
    )


def parse_config(data: bytes) -> DeviceConfig:
    """Parse a 12-byte CONFIG response payload."""
    pp, level, flags, n_lsb, hash_in, blk_sz, ac_tgt = struct.unpack(
        '<BfBBBHH', data[:12]
    )
    return DeviceConfig(
        postprocess=PostProcess(pp),
        initial_level=level,
        startup_test=bool(flags & 1),
        auto_calibration=bool((flags >> 1) & 1),
        repetition_count=bool((flags >> 2) & 1),
        adaptive_proportion=bool((flags >> 3) & 1),
        bit_count=bool((flags >> 4) & 1),
        generate_on_error=bool((flags >> 5) & 1),
        n_lsbits=n_lsb,
        hash_input_size=hash_in,
        block_size=blk_sz,
        autocalibration_target=ac_tgt,
    )


def serialize_config(config: DeviceConfig) -> bytes:
    """Serialize a DeviceConfig to 12 bytes for SET_CONFIG."""
    flags = (
        (config.startup_test & 1)
        | ((config.auto_calibration & 1) << 1)
        | ((config.repetition_count & 1) << 2)
        | ((config.adaptive_proportion & 1) << 3)
        | ((config.bit_count & 1) << 4)
        | ((config.generate_on_error & 1) << 5)
    )
    return struct.pack(
        '<BfBBBHH',
        int(config.postprocess),
        config.initial_level,
        flags,
        config.n_lsbits,
        config.hash_input_size,
        config.block_size,
        config.autocalibration_target,
    )


def parse_statistics(data: bytes) -> DeviceStatistics:
    """Parse a 30-byte STATISTICS response payload."""
    fields = struct.unpack('<QIIIIHf', data[:30])
    return DeviceStatistics(
        generated_bytes=fields[0],
        repetition_count_failures=fields[1],
        adaptive_proportion_failures=fields[2],
        bitcount_failures=fields[3],
        speed=fields[4],
        sensif_average=fields[5],
        ledctrl_level=fields[6],
    )


def checksum8(data: bytes) -> int:
    """Ones-complement 8-bit checksum (for firmware update chunks)."""
    total = sum(data) & 0xFF
    return ~total & 0xFF
