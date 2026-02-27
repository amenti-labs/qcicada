"""Unit tests for ECDSA P-256 certificate and signature verification.

These run without a device — pure crypto logic only.
"""

import pytest
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives import hashes

from qcicada.crypto import (
    verify_certificate,
    verify_signature,
    build_certificate_data,
    parse_hw_version,
    parse_serial_int,
    PUB_KEY_LEN,
    CERTIFICATE_LEN,
)


# -- Test helpers --

def _generate_keypair():
    """Generate an ECDSA P-256 keypair, returning (raw_pub_64, private_key)."""
    private_key = ec.generate_private_key(ec.SECP256R1())
    pub_numbers = private_key.public_key().public_numbers()
    raw_pub = (
        pub_numbers.x.to_bytes(32, 'big') +
        pub_numbers.y.to_bytes(32, 'big')
    )
    return raw_pub, private_key


def _sign(private_key, message: bytes) -> bytes:
    """Sign a message and return raw r || s (64 bytes)."""
    from cryptography.hazmat.primitives.asymmetric import utils
    der_sig = private_key.sign(message, ec.ECDSA(hashes.SHA256()))
    r, s = utils.decode_dss_signature(der_sig)
    return r.to_bytes(32, 'big') + s.to_bytes(32, 'big')


# -- verify_signature --

class TestVerifySignature:
    def test_valid(self):
        pub, priv = _generate_keypair()
        message = b'hello quantum world'
        sig = _sign(priv, message)
        assert verify_signature(pub, message, sig) is True

    def test_wrong_message(self):
        pub, priv = _generate_keypair()
        sig = _sign(priv, b'correct message')
        assert verify_signature(pub, b'wrong message', sig) is False

    def test_wrong_key(self):
        pub1, priv1 = _generate_keypair()
        pub2, _ = _generate_keypair()
        sig = _sign(priv1, b'test data')
        assert verify_signature(pub2, b'test data', sig) is False

    def test_bad_key_length(self):
        with pytest.raises(ValueError, match='64 bytes'):
            verify_signature(b'\x00' * 32, b'msg', b'\x00' * 64)

    def test_bad_sig_length(self):
        pub, _ = _generate_keypair()
        with pytest.raises(ValueError, match='64 bytes'):
            verify_signature(pub, b'msg', b'\x00' * 32)


# -- verify_certificate --

class TestVerifyCertificate:
    def test_valid(self):
        ca_pub, ca_priv = _generate_keypair()
        device_pub = b'\x42' * 64  # dummy device pub key
        hw_major, hw_minor, serial_int = 1, 1, 217

        cert_data = build_certificate_data(hw_major, hw_minor, serial_int, device_pub)
        certificate = _sign(ca_priv, cert_data)

        assert verify_certificate(
            ca_pub, device_pub, certificate, hw_major, hw_minor, serial_int,
        ) is True

    def test_wrong_serial(self):
        ca_pub, ca_priv = _generate_keypair()
        device_pub = b'\x42' * 64
        cert_data = build_certificate_data(1, 1, 217, device_pub)
        certificate = _sign(ca_priv, cert_data)

        assert verify_certificate(
            ca_pub, device_pub, certificate, 1, 1, 999,
        ) is False

    def test_wrong_hw_version(self):
        ca_pub, ca_priv = _generate_keypair()
        device_pub = b'\x42' * 64
        cert_data = build_certificate_data(1, 1, 217, device_pub)
        certificate = _sign(ca_priv, cert_data)

        assert verify_certificate(
            ca_pub, device_pub, certificate, 2, 0, 217,
        ) is False

    def test_wrong_device_pub_key(self):
        ca_pub, ca_priv = _generate_keypair()
        device_pub = b'\x42' * 64
        cert_data = build_certificate_data(1, 1, 217, device_pub)
        certificate = _sign(ca_priv, cert_data)

        wrong_device_pub = b'\x99' * 64
        assert verify_certificate(
            ca_pub, wrong_device_pub, certificate, 1, 1, 217,
        ) is False

    def test_bad_ca_key_length(self):
        with pytest.raises(ValueError):
            verify_certificate(b'\x00' * 32, b'\x00' * 64, b'\x00' * 64, 1, 1, 1)

    def test_bad_device_key_length(self):
        with pytest.raises(ValueError):
            verify_certificate(b'\x00' * 64, b'\x00' * 32, b'\x00' * 64, 1, 1, 1)

    def test_bad_certificate_length(self):
        with pytest.raises(ValueError):
            verify_certificate(b'\x00' * 64, b'\x00' * 64, b'\x00' * 32, 1, 1, 1)


# -- build_certificate_data --

class TestBuildCertificateData:
    def test_format(self):
        pub_key = b'\xAA' * 64
        data = build_certificate_data(1, 2, 217, pub_key)
        # u16(0) + u8(1) + u8(2) + u32_le(217) + 64 bytes = 72 bytes
        assert len(data) == 72
        assert data[0:2] == b'\x00\x00'  # reserved
        assert data[2] == 1              # hw_major
        assert data[3] == 2              # hw_minor
        import struct
        assert struct.unpack('<I', data[4:8])[0] == 217
        assert data[8:] == pub_key

    def test_large_serial(self):
        import struct
        data = build_certificate_data(3, 5, 999999, b'\x00' * 64)
        assert struct.unpack('<I', data[4:8])[0] == 999999


# -- parse_hw_version --

class TestParseHwVersion:
    def test_normal(self):
        assert parse_hw_version('CICADA-QRNG-1.1') == (1, 1)

    def test_different_version(self):
        assert parse_hw_version('CICADA-QRNG-2.3') == (2, 3)

    def test_wrong_prefix(self):
        assert parse_hw_version('SOME-OTHER-1.1') is None

    def test_no_dot(self):
        assert parse_hw_version('CICADA-QRNG-11') is None

    def test_non_numeric(self):
        assert parse_hw_version('CICADA-QRNG-a.b') is None


# -- parse_serial_int --

class TestParseSerialInt:
    def test_normal(self):
        assert parse_serial_int('QC0000000217') == 217

    def test_large(self):
        assert parse_serial_int('QC0000999999') == 999999

    def test_wrong_prefix(self):
        assert parse_serial_int('XX0000000217') is None

    def test_non_numeric(self):
        assert parse_serial_int('QCabcdef') is None


# -- Protocol constants for custom commands --

class TestCustomCommandConstants:
    def test_custom_command_codes(self):
        from qcicada.protocol import (
            CMD_GET_DEV_PUB_KEY, CMD_REBOOT, CMD_GET_DEV_CERTIFICATE,
        )
        assert CMD_GET_DEV_PUB_KEY == b'\xF7'
        assert CMD_REBOOT == b'\xF8'
        assert CMD_GET_DEV_CERTIFICATE == b'\xF9'

    def test_custom_response_codes(self):
        from qcicada.protocol import (
            RESP_CUSTOM_OK, RESP_DEV_PUB_KEY, RESP_DEV_CERTIFICATE,
        )
        assert RESP_CUSTOM_OK == b'\xF8'
        assert RESP_DEV_PUB_KEY == b'\xF9'
        assert RESP_DEV_CERTIFICATE == b'\xFB'

    def test_custom_response_mappings(self):
        from qcicada.protocol import (
            CMD_GET_DEV_PUB_KEY, CMD_REBOOT, CMD_GET_DEV_CERTIFICATE,
            RESP_CUSTOM_OK, RESP_DEV_PUB_KEY, RESP_DEV_CERTIFICATE,
            SUCCESS_RESPONSE, PAYLOAD_SIZE,
        )
        assert SUCCESS_RESPONSE[CMD_GET_DEV_PUB_KEY] == RESP_DEV_PUB_KEY
        assert SUCCESS_RESPONSE[CMD_REBOOT] == RESP_CUSTOM_OK
        assert SUCCESS_RESPONSE[CMD_GET_DEV_CERTIFICATE] == RESP_DEV_CERTIFICATE
        assert PAYLOAD_SIZE[RESP_DEV_PUB_KEY] == 64
        assert PAYLOAD_SIZE[RESP_DEV_CERTIFICATE] == 64
        assert PAYLOAD_SIZE[RESP_CUSTOM_OK] == 0

    def test_build_reboot(self):
        from qcicada.protocol import build_reboot, CMD_REBOOT, CUSTOM_MAGIC
        frame = build_reboot()
        assert frame[0:1] == CMD_REBOOT
        assert frame[1:3] == CUSTOM_MAGIC
        assert len(frame) == 3
