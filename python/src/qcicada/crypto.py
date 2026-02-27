"""ECDSA P-256 certificate and signature verification for QCicada devices.

The QCicada QRNG has an internal ECDSA P-256 keypair. A Certificate Authority
(CA) signs a blob containing the device's hardware version, serial number, and
public key. This module verifies that chain:

1. Certificate verification: Confirm the device's public key is CA-signed.
2. Signature verification: Confirm signed-read data was produced by the device.
"""

from __future__ import annotations

import struct

from cryptography.hazmat.primitives.asymmetric import ec, utils
from cryptography.hazmat.primitives import hashes

PUB_KEY_LEN = 64
CERTIFICATE_LEN = 64


def build_certificate_data(
    hw_major: int, hw_minor: int, serial_int: int, pub_key: bytes,
) -> bytes:
    """Build the certificate data blob that gets signed/verified.

    Format: ``u16(0) || u8(hw_major) || u8(hw_minor) || u32_le(serial_int) || pub_key[64]``
    """
    return struct.pack('<HBBI', 0, hw_major, hw_minor, serial_int) + pub_key


def parse_hw_version(hw_info: str) -> tuple[int, int] | None:
    """Parse a hardware info string like ``"CICADA-QRNG-1.1"`` into ``(major, minor)``."""
    prefix = 'CICADA-QRNG-'
    if not hw_info.startswith(prefix):
        return None
    parts = hw_info[len(prefix):].split('.')
    if len(parts) < 2:
        return None
    try:
        return int(parts[0]), int(parts[1])
    except ValueError:
        return None


def parse_serial_int(serial: str) -> int | None:
    """Parse a serial string like ``"QC0000000217"`` into the integer ``217``."""
    if not serial.startswith('QC'):
        return None
    try:
        return int(serial[2:])
    except ValueError:
        return None


def _load_pub_key(raw: bytes) -> ec.EllipticCurvePublicKey:
    """Load a raw 64-byte public key (x || y) into an EC public key object."""
    # Uncompressed point format: 0x04 || x[32] || y[32]
    uncompressed = b'\x04' + raw
    return ec.EllipticCurvePublicKey.from_encoded_point(ec.SECP256R1(), uncompressed)


def _parse_signature(raw: bytes) -> bytes:
    """Convert raw r || s (32+32 bytes) to DER-encoded signature."""
    r = int.from_bytes(raw[:32], 'big')
    s = int.from_bytes(raw[32:], 'big')
    return utils.encode_dss_signature(r, s)


def verify_certificate(
    ca_pub_key: bytes,
    device_pub_key: bytes,
    certificate: bytes,
    hw_major: int,
    hw_minor: int,
    serial_int: int,
) -> bool:
    """Verify a device certificate against a CA public key.

    Args:
        ca_pub_key: 64 bytes (x || y) of the CA public key.
        device_pub_key: 64 bytes of the device's public key.
        certificate: 64 bytes (r || s) of the CA's ECDSA signature.
        hw_major: Hardware major version.
        hw_minor: Hardware minor version.
        serial_int: Numeric serial number.

    Returns:
        True if the certificate is valid, False otherwise.

    Raises:
        ValueError: If key/certificate lengths are wrong.
    """
    if len(ca_pub_key) != PUB_KEY_LEN:
        raise ValueError(f'CA public key must be {PUB_KEY_LEN} bytes, got {len(ca_pub_key)}')
    if len(device_pub_key) != PUB_KEY_LEN:
        raise ValueError(f'Device public key must be {PUB_KEY_LEN} bytes, got {len(device_pub_key)}')
    if len(certificate) != CERTIFICATE_LEN:
        raise ValueError(f'Certificate must be {CERTIFICATE_LEN} bytes, got {len(certificate)}')

    message = build_certificate_data(hw_major, hw_minor, serial_int, device_pub_key)
    return verify_signature(ca_pub_key, message, certificate)


def verify_signature(pub_key: bytes, message: bytes, signature: bytes) -> bool:
    """Verify an ECDSA-SHA256 signature.

    Args:
        pub_key: 64 bytes (x || y) of the signer's P-256 public key.
        message: The signed data.
        signature: 64 bytes (r || s) in big-endian.

    Returns:
        True if valid, False otherwise.

    Raises:
        ValueError: If key/signature lengths are wrong.
    """
    if len(pub_key) != PUB_KEY_LEN:
        raise ValueError(f'Public key must be {PUB_KEY_LEN} bytes, got {len(pub_key)}')
    if len(signature) != CERTIFICATE_LEN:
        raise ValueError(f'Signature must be {CERTIFICATE_LEN} bytes, got {len(signature)}')

    try:
        key = _load_pub_key(pub_key)
        der_sig = _parse_signature(signature)
        key.verify(der_sig, message, ec.ECDSA(hashes.SHA256()))
        return True
    except Exception:
        return False
