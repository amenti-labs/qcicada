"""High-level QCicada QRNG device interface."""

from __future__ import annotations

import logging
from dataclasses import replace

from .protocol import (
    CMD_GET_CONFIG, CMD_GET_INFO, CMD_GET_STATISTICS, CMD_GET_STATUS,
    CMD_GET_DEV_PUB_KEY, CMD_GET_DEV_CERTIFICATE, CMD_REBOOT,
    CMD_RESET, CMD_SET_CONFIG, CMD_SIGNED_READ, CMD_START, CMD_STOP,
    RESP_ACK, RESP_NACK, SUCCESS_RESPONSE, PAYLOAD_SIZE,
    MAX_BLOCK_SIZE, SIGNATURE_LEN, PUB_KEY_LEN, CERTIFICATE_LEN,
    build_cmd, build_start_one_shot, build_start_continuous,
    build_signed_read, build_reboot, serialize_config,
    parse_config, parse_info, parse_statistics, parse_status,
)
from .crypto import (
    verify_certificate as _verify_certificate,
    verify_signature as _verify_signature,
    parse_hw_version, parse_serial_int,
)
from .serial import SerialTransport, find_devices
from .types import (
    DeviceConfig, DeviceInfo, DeviceStatistics, DeviceStatus, PostProcess,
    SignedRead,
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

    def signed_read(self, n: int) -> SignedRead:
        """Get ``n`` random bytes with a 64-byte cryptographic signature.

        Requires QCicada firmware 5.13+. The signature is produced by the
        device's internal asymmetric key.

        Args:
            n: Number of random bytes to read (1-65535).

        Raises:
            QCicadaError: If the device fails to respond.
            ValueError: If n is out of range.
        """
        if not 1 <= n <= 65535:
            raise ValueError(f'n must be 1-65535, got {n}')
        frame = build_signed_read(n)
        result = self._command(CMD_SIGNED_READ, frame[1:])
        if result is None:
            raise QCicadaError('Failed to start signed read')
        # Read random data + 64-byte signature
        total = n + SIGNATURE_LEN
        self._transport.set_timeout(0.5 + 0.0001 * n)
        try:
            buf = self._transport.read(total)
        except Exception as exc:
            raise QCicadaError(f'Failed reading signed data: {exc}') from exc
        if len(buf) != total:
            raise QCicadaError(f'Expected {total} bytes (data+sig), got {len(buf)}')
        return SignedRead(data=buf[:n], signature=buf[n:])

    def start_continuous(self) -> None:
        """Start continuous random data generation.

        After calling this, use :meth:`read_continuous` to read streaming data.
        Call :meth:`stop` to end continuous mode.

        Raises:
            QCicadaError: If the device rejects the command.
        """
        frame = build_start_continuous()
        result = self._command(CMD_START, frame[1:])
        if result is None:
            raise QCicadaError('Failed to start continuous mode')

    def read_continuous(self, n: int) -> bytes:
        """Read bytes from an active continuous mode stream.

        Call :meth:`start_continuous` first. Returns exactly ``n`` bytes.

        Args:
            n: Number of bytes to read.

        Raises:
            QCicadaError: If the read fails or times out.
        """
        if n == 0:
            return b''
        self._transport.set_timeout(0.5 + 0.0001 * n)
        try:
            data = self._transport.read(n)
        except Exception as exc:
            raise QCicadaError(f'Failed reading continuous data: {exc}') from exc
        if len(data) != n:
            raise QCicadaError(f'Expected {n} continuous bytes, got {len(data)}')
        return data

    def get_dev_pub_key(self) -> bytes:
        """Retrieve the device's ECDSA P-256 public key (64 bytes: x || y).

        Requires QCicada firmware with certificate support.
        """
        data = self._command(CMD_GET_DEV_PUB_KEY)
        if data is None:
            raise QCicadaError('Failed to get device public key (NACK)')
        if len(data) != PUB_KEY_LEN:
            raise QCicadaError(
                f'Expected {PUB_KEY_LEN} byte public key, got {len(data)}'
            )
        return data

    def get_dev_certificate(self) -> bytes:
        """Retrieve the device certificate (64 bytes: ECDSA r || s).

        This is the CA's signature over the device's identity (hw version,
        serial number, and public key).
        """
        data = self._command(CMD_GET_DEV_CERTIFICATE)
        if data is None:
            raise QCicadaError('Failed to get device certificate (NACK)')
        if len(data) != CERTIFICATE_LEN:
            raise QCicadaError(
                f'Expected {CERTIFICATE_LEN} byte certificate, got {len(data)}'
            )
        return data

    def get_verified_pub_key(self, ca_pub_key: bytes) -> bytes:
        """Retrieve and verify the device's public key against a CA public key.

        Fetches the device's info, public key, and certificate, then verifies
        the certificate chain. Returns the verified public key on success.

        Args:
            ca_pub_key: 64 bytes (x || y) of the Certificate Authority's public key.

        Raises:
            QCicadaError: If verification fails or device communication fails.
        """
        info = self.get_info()
        dev_pub_key = self.get_dev_pub_key()
        certificate = self.get_dev_certificate()

        hw_ver = parse_hw_version(info.hw_info)
        if hw_ver is None:
            raise QCicadaError(
                f"Cannot parse hardware version from '{info.hw_info}'"
            )
        hw_major, hw_minor = hw_ver

        serial_int = parse_serial_int(info.serial)
        if serial_int is None:
            raise QCicadaError(
                f"Cannot parse serial number from '{info.serial}'"
            )

        valid = _verify_certificate(
            ca_pub_key, dev_pub_key, certificate,
            hw_major, hw_minor, serial_int,
        )
        if not valid:
            raise QCicadaError('Device certificate verification failed')
        return dev_pub_key

    def signed_read_verified(self, n: int, device_pub_key: bytes) -> SignedRead:
        """Perform a signed read and verify the signature.

        Args:
            n: Number of random bytes to read (1-65535).
            device_pub_key: 64 bytes (x || y) of the device's verified public key.

        Returns:
            Verified :class:`SignedRead` with data and signature.

        Raises:
            QCicadaError: If verification fails.
        """
        result = self.signed_read(n)
        valid = _verify_signature(device_pub_key, result.data, result.signature)
        if not valid:
            raise QCicadaError('Signed read signature verification failed')
        return result

    def reboot(self) -> None:
        """Reboot the device.

        The device will disconnect and reconnect — you must re-open the
        connection after calling this.
        """
        frame = build_reboot()
        self._transport.flush()
        try:
            self._transport.write(frame)
        except IOError:
            pass  # Device may disconnect immediately
        # Try to read optional response
        try:
            self._transport.set_timeout(0.5)
            self._transport.read(1)
        except Exception:
            pass

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
