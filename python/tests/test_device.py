"""Integration tests — require a physical QCicada device.

Run with: pytest python/tests/test_device.py
Skipped automatically if no device is connected.
Override with: QCICADA_PORT=/dev/cu.usbserial-DK0HFP4T pytest python/tests/test_device.py
"""

import pytest
from qcicada import (
    QCicada, QCicadaError, PostProcess, SignedRead,
    find_devices, probe_device, discover_devices,
)
from dataclasses import replace


pytestmark = pytest.mark.device


class TestDeviceInfo:
    def test_get_info(self, qrng):
        info = qrng.get_info()
        assert info.serial, "serial should not be empty"
        assert info.hw_info, "hw_info should not be empty"
        assert info.fw_version > 0
        assert info.core_version > 0

    def test_get_status(self, qrng):
        status = qrng.get_status()
        assert status.initialized is True
        assert status.startup_test_in_progress is False
        assert status.voltage_low is False
        assert status.voltage_high is False
        assert status.ready_bytes > 0

    def test_get_statistics(self, qrng):
        stats = qrng.get_statistics()
        assert stats.speed > 0


class TestConfig:
    def test_get_config(self, qrng):
        config = qrng.get_config()
        assert config.block_size > 0

    def test_config_roundtrip(self, qrng):
        original = qrng.get_config()

        modified = replace(original, postprocess=PostProcess.RAW_NOISE)
        qrng.set_config(modified)

        readback = qrng.get_config()
        assert readback.postprocess == PostProcess.RAW_NOISE

        # Restore
        qrng.set_config(original)
        restored = qrng.get_config()
        assert restored.postprocess == original.postprocess

    def test_set_postprocess_preserves_other_fields(self, qrng):
        original = qrng.get_config()
        qrng.set_postprocess(PostProcess.RAW_NOISE)
        readback = qrng.get_config()
        assert readback.postprocess == PostProcess.RAW_NOISE
        assert readback.block_size == original.block_size
        assert readback.auto_calibration == original.auto_calibration
        # Restore
        qrng.set_postprocess(original.postprocess)


class TestRandom:
    def test_sha256_32_bytes(self, qrng):
        qrng.set_postprocess(PostProcess.SHA256)
        data = qrng.random(32)
        assert len(data) == 32
        assert any(b != 0 for b in data), "should not be all zeros"

    def test_different_each_time(self, qrng):
        a = qrng.random(32)
        b = qrng.random(32)
        assert a != b

    def test_raw_noise(self, qrng):
        qrng.set_postprocess(PostProcess.RAW_NOISE)
        data = qrng.random(32)
        assert len(data) == 32
        assert any(b != 0 for b in data)
        qrng.set_postprocess(PostProcess.SHA256)

    def test_raw_samples(self, qrng):
        qrng.set_postprocess(PostProcess.RAW_SAMPLES)
        data = qrng.random(32)
        assert len(data) == 32
        qrng.set_postprocess(PostProcess.SHA256)

    def test_various_sizes(self, qrng):
        for size in [1, 16, 32, 64, 128, 256, 512, 1024]:
            data = qrng.random(size)
            assert len(data) == size, f"wrong length for size {size}"

    def test_zero_returns_empty(self, qrng):
        data = qrng.random(0)
        assert data == b''

    def test_invalid_size_raises(self, qrng):
        with pytest.raises(ValueError):
            qrng.random(70000)
        with pytest.raises(ValueError):
            qrng.random(-1)

    def test_fill_bytes(self, qrng):
        buf = bytearray(256)
        qrng.fill_bytes(buf)
        assert any(b != 0 for b in buf)


class TestSignedRead:
    def test_signed_read_32_bytes(self, qrng):
        qrng.set_postprocess(PostProcess.SHA256)
        result = qrng.signed_read(32)
        assert isinstance(result, SignedRead)
        assert len(result.data) == 32
        assert len(result.signature) == 64
        assert any(b != 0 for b in result.data), "data should not be all zeros"
        assert any(b != 0 for b in result.signature), "signature should not be all zeros"

    def test_signed_read_different_each_time(self, qrng):
        a = qrng.signed_read(32)
        b = qrng.signed_read(32)
        assert a.data != b.data
        assert a.signature != b.signature

    def test_signed_read_invalid_size(self, qrng):
        with pytest.raises(ValueError):
            qrng.signed_read(0)
        with pytest.raises(ValueError):
            qrng.signed_read(-1)
        with pytest.raises(ValueError):
            qrng.signed_read(70000)


class TestContinuousMode:
    def test_continuous_read(self, qrng):
        qrng.start_continuous()
        data = qrng.read_continuous(64)
        assert len(data) == 64
        assert any(b != 0 for b in data)
        qrng.stop()
        status = qrng.get_status()
        assert status.initialized

    def test_continuous_multiple_reads(self, qrng):
        qrng.start_continuous()
        a = qrng.read_continuous(32)
        b = qrng.read_continuous(32)
        assert a != b
        qrng.stop()

    def test_continuous_zero_returns_empty(self, qrng):
        qrng.start_continuous()
        data = qrng.read_continuous(0)
        assert data == b''
        qrng.stop()


class TestStop:
    def test_stop_is_safe(self, qrng):
        qrng.stop()
        # Device should still work after stop
        status = qrng.get_status()
        assert status.initialized


class TestDiscovery:
    def test_find_devices_returns_list(self):
        ports = find_devices()
        assert isinstance(ports, list)
        # At least one device if we're running device tests
        assert len(ports) > 0

    def test_discover_devices(self):
        devices = discover_devices()
        assert len(devices) > 0
        dev = devices[0]
        assert dev.info.serial
        assert dev.port

    def test_probe_known_port(self):
        ports = find_devices()
        assert len(ports) > 0
        info = probe_device(ports[0])
        assert info is not None
        assert info.serial

    def test_probe_bogus_port(self):
        info = probe_device('/dev/nonexistent_port_xyz')
        assert info is None


class TestContextManager:
    def test_with_statement(self):
        ports = find_devices()
        if not ports:
            pytest.skip("No device")
        with QCicada(port=ports[0]) as qrng:
            data = qrng.random(32)
            assert len(data) == 32
        # After exit, device should be closed (no crash)


class TestErrorHandling:
    def test_no_device_raises(self):
        with pytest.raises(QCicadaError):
            QCicada(port='/dev/nonexistent_port_xyz')

    def test_error_message_includes_port(self):
        try:
            QCicada(port='/dev/nonexistent_port_xyz')
        except QCicadaError as e:
            assert '/dev/nonexistent_port_xyz' in str(e)
