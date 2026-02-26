"""Device discovery: find and identify QCicada QRNG devices."""

from __future__ import annotations

from dataclasses import dataclass

from .device import QCicada, QCicadaError
from .serial import find_devices
from .types import DeviceInfo


@dataclass
class DiscoveredDevice:
    """A discovered QCicada device with its port and identification."""
    port: str
    info: DeviceInfo


def probe_device(port: str) -> DeviceInfo | None:
    """Probe a specific port to check if a QCicada device is connected.

    Opens the port, sends GET_INFO, and returns the device info if it responds
    correctly. Returns None if the port is not a QCicada (or is unavailable).
    """
    try:
        with QCicada(port=port) as dev:
            return dev.get_info()
    except (QCicadaError, OSError):
        return None


def discover_devices() -> list[DiscoveredDevice]:
    """Discover all connected QCicada devices.

    Scans matching serial ports and probes each one with GET_INFO to verify
    it's actually a QCicada. Returns only confirmed devices with their info.

    This is slower than ``find_devices()`` (which only checks port names) but
    guarantees the returned ports are real QCicada devices.
    """
    result = []
    for port in find_devices():
        info = probe_device(port)
        if info is not None:
            result.append(DiscoveredDevice(port=port, info=info))
    return result


def open_by_serial(serial: str) -> QCicada:
    """Open a QCicada device by its serial number.

    Probes all matching ports and opens the one whose serial number matches.

    Args:
        serial: Device serial number string (e.g. from DeviceInfo.serial).

    Raises:
        QCicadaError: If no device with that serial number is found.
    """
    for port in find_devices():
        info = probe_device(port)
        if info is not None and info.serial == serial:
            return QCicada(port=port)
    raise QCicadaError(f'No QCicada device found with serial: {serial}')
