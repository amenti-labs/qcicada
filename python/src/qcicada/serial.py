"""Platform-aware serial transport for QCicada QRNG devices."""

from __future__ import annotations

import glob
import sys
import time

import serial as pyserial


def find_devices() -> list[str]:
    """Auto-discover QCicada QRNG devices by serial port pattern.

    Returns a list of matching device paths.
    """
    if sys.platform == 'darwin':
        return sorted(glob.glob('/dev/cu.usbserial-*'))
    else:
        return sorted(glob.glob('/dev/ttyUSB*'))


class SerialTransport:
    """Serial transport with macOS FTDI workarounds.

    On macOS:
      - No inter_byte_timeout (causes read failures with FTDI driver)
      - Minimum 0.5s read timeout
      - Flush + 50ms delay after every write (FTDI latency)
      - Uses /dev/cu.* ports (not /dev/tty.*)

    On Linux:
      - Standard pyserial behavior with inter_byte_timeout
    """

    MIN_TIMEOUT_MACOS = 0.5

    def __init__(self, port: str, timeout: float = 2.0):
        self._is_macos = sys.platform == 'darwin'

        kwargs: dict = dict(
            baudrate=1_000_000,
            timeout=timeout,
            write_timeout=1,
        )
        if not self._is_macos:
            kwargs['inter_byte_timeout'] = 0.1

        self._ser = pyserial.Serial(port, **kwargs)

        # Stop any continuous mode left over and drain the buffer
        self._ser.write(bytes([0x05]))
        time.sleep(0.5)
        self._ser.timeout = 0.3
        while self._ser.read(4096):
            pass
        self._ser.reset_input_buffer()
        self._ser.timeout = timeout
        time.sleep(0.1)

    def write(self, data: bytes) -> int:
        """Write data. On macOS, flushes and waits for FTDI latency."""
        n = self._ser.write(data)
        if self._is_macos:
            self._ser.flush()
            time.sleep(0.05)
        return n

    def read(self, length: int) -> bytes:
        """Read exactly `length` bytes (or fewer on timeout)."""
        return self._ser.read(length)

    def flush(self) -> None:
        """Flush output and clear input buffer."""
        self._ser.flush()
        self._ser.reset_input_buffer()

    def set_timeout(self, timeout: float) -> None:
        """Set read timeout, enforcing macOS minimum."""
        if self._is_macos:
            timeout = max(timeout, self.MIN_TIMEOUT_MACOS)
        self._ser.timeout = timeout

    def close(self) -> None:
        """Close the serial port."""
        self._ser.close()
