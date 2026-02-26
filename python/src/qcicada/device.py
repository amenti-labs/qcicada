"""High-level QCicada QRNG device interface."""

from __future__ import annotations

import logging
from dataclasses import replace

from .protocol import (
    CMD_GET_CONFIG, CMD_GET_INFO, CMD_GET_STATISTICS, CMD_GET_STATUS,
    CMD_RESET, CMD_SET_CONFIG, CMD_START, CMD_STOP,
    RESP_ACK, RESP_NACK, SUCCESS_RESPONSE, PAYLOAD_SIZE,
    MAX_BLOCK_SIZE,
    build_cmd, build_start_one_shot, serialize_config,
    parse_config, parse_info, parse_statistics, parse_status,
)
from .serial import SerialTransport, find_devices
from .types import (
    DeviceConfig, DeviceInfo, DeviceStatistics, DeviceStatus, PostProcess,
)

logger = logging.getLogger(__name__)


class QCicadaError(Exception):
    """Raised on unexpected protocol responses or communication failures."""


class QCicada:
    """High-level interface to a QCicada QRNG device.

    Usage::

        with QCicada() as qrng:
            print(qrng.random(32).hex())
    """

    def __init__(self, port: str | None = None, timeout: float = 2.0):
        """Connect to a QCicada device.

        Args:
            port: Serial port path, e.g. ``"/dev/cu.usbserial-DK0HFP4T"``.
                  If None, auto-discovers the first available device.
            timeout: Default read timeout in seconds.

        Raises:
            QCicadaError: If no device is found or the serial port cannot be opened.
        """
        if port is None:
            devices = find_devices()
            if not devices:
                raise QCicadaError('No QCicada device found')
            port = devices[0]
        try:
            self._transport = SerialTransport(port, timeout=timeout)
        except Exception as exc:
            raise QCicadaError(f'Failed to open {port}: {exc}') from exc

    # --- Public API ---

    def get_info(self) -> DeviceInfo:
        """Read device identification (version, serial, hardware)."""
        data = self._command(CMD_GET_INFO)
        if data is None:
            raise QCicadaError('Failed to get device info')
        return parse_info(data)

    def get_status(self) -> DeviceStatus:
        """Read current device status."""
        data = self._command(CMD_GET_STATUS)
        if data is None:
            raise QCicadaError('Failed to get device status')
        return parse_status(data)

    def get_config(self) -> DeviceConfig:
        """Read current device configuration."""
        data = self._command(CMD_GET_CONFIG)
        if data is None:
            raise QCicadaError('Failed to get device config')
        return parse_config(data)

    def set_config(self, config: DeviceConfig) -> None:
        """Write a full device configuration.

        Raises:
            QCicadaError: If the device rejects the configuration.
        """
        payload = serialize_config(config)
        result = self._command(CMD_SET_CONFIG, payload)
        if result is None:
            raise QCicadaError('Failed to set device config (NACK)')

    def get_statistics(self) -> DeviceStatistics:
        """Read generation statistics since last reset."""
        data = self._command(CMD_GET_STATISTICS)
        if data is None:
            raise QCicadaError('Failed to get device statistics')
        return parse_statistics(data)

    def random(self, n: int) -> bytes:
        """Get ``n`` random bytes using one-shot mode.

        Args:
            n: Number of bytes to read (1-65535).

        Raises:
            QCicadaError: If the device fails to respond.
            ValueError: If n is out of range.
        """
        if n == 0:
            return b''
        if not 1 <= n <= 65535:
            raise ValueError(f'n must be 1-65535, got {n}')
        frame = build_start_one_shot(n)
        result = self._command(CMD_START, frame[1:])
        if result is None:
            raise QCicadaError('Failed to start one-shot read')
        # Read the random data
        self._transport.set_timeout(0.5 + 0.0001 * n)
        try:
            data = self._transport.read(n)
        except Exception as exc:
            raise QCicadaError(f'Failed reading random data: {exc}') from exc
        if len(data) != n:
            raise QCicadaError(f'Expected {n} bytes, got {len(data)}')
        return data

    def fill_bytes(self, buf: bytearray) -> None:
        """Fill a buffer with random bytes, chunking as needed.

        Handles reads larger than the 65535-byte protocol limit automatically.

        Args:
            buf: Mutable buffer to fill with random data.
        """
        offset = 0
        while offset < len(buf):
            chunk = min(len(buf) - offset, 65535)
            data = self.random(chunk)
            buf[offset:offset + len(data)] = data
            offset += len(data)

    def set_postprocess(self, mode: PostProcess) -> None:
        """Change post-processing mode, preserving other config settings.

        Raises:
            QCicadaError: If the device rejects the change.
        """
        config = self.get_config()
        config = replace(config, postprocess=mode)
        self.set_config(config)

    def reset(self) -> None:
        """Reset the device (restarts startup test and clears statistics).

        Raises:
            QCicadaError: If the device rejects the reset.
        """
        result = self._command(CMD_RESET)
        if result is None:
            raise QCicadaError('Failed to reset device (NACK)')

    def stop(self) -> None:
        """Send STOP command to halt any active generation."""
        self._command(CMD_STOP)

    def close(self) -> None:
        """Close the serial connection."""
        self._transport.close()

    def __enter__(self) -> QCicada:
        return self

    def __exit__(self, *args) -> None:
        self.close()

    # --- Internal protocol handling ---

    def _command(self, cmd_code: bytes, payload: bytes | None = None):
        """Send a command and read the response.

        Returns the response payload bytes, True (for empty-payload success),
        or None on NACK.

        Raises QCicadaError on write failures or unexpected response bytes.
        """
        if cmd_code not in SUCCESS_RESPONSE:
            raise ValueError(f'Unknown command code: {cmd_code.hex()}')

        self._transport.flush()
        frame = build_cmd(cmd_code, payload)

        try:
            self._transport.write(frame)
        except IOError as exc:
            raise QCicadaError(f'Write failed: {exc}') from exc

        # STOP command: drain buffer, find ACK near end
        if cmd_code == CMD_STOP:
            return self._handle_stop()

        # Normal command: read 1-byte response code
        try:
            self._transport.set_timeout(3)
            resp = self._transport.read(1)
            if len(resp) == 0:
                return None
        except Exception as exc:
            raise QCicadaError(f'Read failed: {exc}') from exc

        expected = SUCCESS_RESPONSE[cmd_code]
        if resp == expected:
            expected_size = PAYLOAD_SIZE[expected]
            if expected_size == 0:
                return True
            self._transport.set_timeout(max(0.5, 0.001 * expected_size))
            try:
                resp_payload = self._transport.read(expected_size)
            except Exception as exc:
                raise QCicadaError(f'Read payload failed: {exc}') from exc
            if len(resp_payload) != expected_size:
                return None
            return resp_payload
        elif resp == RESP_NACK:
            return None
        else:
            raise QCicadaError(f'Unexpected response byte: 0x{resp.hex()}')

    def _handle_stop(self):
        """Handle STOP command response: drain pipe and find ACK."""
        ack_payload_size = PAYLOAD_SIZE[RESP_ACK]
        drain_size = MAX_BLOCK_SIZE * 2 + ack_payload_size + 1

        self._transport.set_timeout(0.5)
        for _ in range(2):
            resp = self._transport.read(drain_size)
            if len(resp) == 1 and resp == RESP_NACK:
                return None
            if len(resp) < ack_payload_size + 1:
                return None
            # Check for ACK at expected position from end
            ack_pos = len(resp) - 1 - ack_payload_size
            if resp[ack_pos] == RESP_ACK[0]:
                return True
        return None
