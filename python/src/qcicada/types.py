"""Data types for the QCicada QRNG protocol."""

from dataclasses import dataclass
from enum import IntEnum


class PostProcess(IntEnum):
    """Post-processing mode for random data output."""
    SHA256 = 0        # NIST SP 800-90B compliant SHA256 conditioning
    RAW_NOISE = 1     # Raw noise after health-test conditioning
    RAW_SAMPLES = 2   # Raw samples directly from QOM, no conditioning


@dataclass
class DeviceInfo:
    """Device identification and version information."""
    core_version: int
    fw_version: int
    serial: str
    hw_info: str


@dataclass
class DeviceStatus:
    """Current device operational status."""
    initialized: bool
    startup_test_in_progress: bool
    voltage_low: bool
    voltage_high: bool
    voltage_undefined: bool
    bitcount: bool
    repetition_count: bool
    adaptive_proportion: bool
    ready_bytes: int


@dataclass
class DeviceConfig:
    """Device configuration (full mode)."""
    postprocess: PostProcess
    initial_level: float
    startup_test: bool
    auto_calibration: bool
    repetition_count: bool
    adaptive_proportion: bool
    bit_count: bool
    generate_on_error: bool
    n_lsbits: int
    hash_input_size: int
    block_size: int
    autocalibration_target: int


@dataclass
class DeviceStatistics:
    """Device generation statistics since last reset."""
    generated_bytes: int
    repetition_count_failures: int
    adaptive_proportion_failures: int
    bitcount_failures: int
    speed: int
    sensif_average: int
    ledctrl_level: float
