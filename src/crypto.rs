//! ECDSA P-256 certificate and signature verification for QCicada devices.
//!
//! The QCicada QRNG has an internal ECDSA P-256 keypair. A Certificate Authority
//! (CA) signs a blob containing the device's hardware version, serial number, and
//! public key. This module verifies that chain:
//!
//! 1. **Certificate verification**: Confirm the device's public key is CA-signed.
//! 2. **Signature verification**: Confirm signed-read data was produced by the device.

use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use p256::EncodedPoint;

use crate::protocol::{build_certificate_data, CERTIFICATE_LEN, PUB_KEY_LEN};

/// Verify a device certificate against a CA public key.
///
/// The certificate is an ECDSA-SHA256 signature over:
/// `u16(0) || u8(hw_major) || u8(hw_minor) || u32_le(serial_int) || pub_key[64]`
///
/// # Arguments
/// - `ca_pub_key`: 64 bytes (uncompressed x || y) of the CA public key.
/// - `device_pub_key`: 64 bytes of the device's public key.
/// - `certificate`: 64 bytes (r || s) of the CA's signature.
/// - `hw_major`, `hw_minor`: Hardware version from `DeviceInfo.hw_info`.
/// - `serial_int`: Numeric serial from `DeviceInfo.serial` (e.g. 217 from "QC0000000217").
pub fn verify_certificate(
    ca_pub_key: &[u8],
    device_pub_key: &[u8],
    certificate: &[u8],
    hw_major: u8,
    hw_minor: u8,
    serial_int: u32,
) -> Result<bool, String> {
    if ca_pub_key.len() != PUB_KEY_LEN {
        return Err(format!(
            "CA public key must be {} bytes, got {}",
            PUB_KEY_LEN,
            ca_pub_key.len()
        ));
    }
    if device_pub_key.len() != PUB_KEY_LEN {
        return Err(format!(
            "Device public key must be {} bytes, got {}",
            PUB_KEY_LEN,
            device_pub_key.len()
        ));
    }
    if certificate.len() != CERTIFICATE_LEN {
        return Err(format!(
            "Certificate must be {} bytes, got {}",
            CERTIFICATE_LEN,
            certificate.len()
        ));
    }

    let message = build_certificate_data(hw_major, hw_minor, serial_int, device_pub_key);
    verify_ecdsa_p256(ca_pub_key, &message, certificate)
}

/// Verify an ECDSA-SHA256 signature over data using a raw P-256 public key.
///
/// # Arguments
/// - `pub_key`: 64 bytes (x || y) of the signer's uncompressed P-256 public key.
/// - `message`: The signed data.
/// - `signature`: 64 bytes (r || s) in big-endian.
pub fn verify_signature(
    pub_key: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    if pub_key.len() != PUB_KEY_LEN {
        return Err(format!(
            "Public key must be {} bytes, got {}",
            PUB_KEY_LEN,
            pub_key.len()
        ));
    }
    if signature.len() != CERTIFICATE_LEN {
        return Err(format!(
            "Signature must be {} bytes, got {}",
            CERTIFICATE_LEN,
            signature.len()
        ));
    }

    verify_ecdsa_p256(pub_key, message, signature)
}

/// Internal: verify ECDSA-SHA256 with raw key/sig bytes.
fn verify_ecdsa_p256(
    pub_key_raw: &[u8],
    message: &[u8],
    sig_raw: &[u8],
) -> Result<bool, String> {
    // Build uncompressed point: 0x04 || x[32] || y[32]
    let mut uncompressed = vec![0x04];
    uncompressed.extend_from_slice(pub_key_raw);
    let point =
        EncodedPoint::from_bytes(&uncompressed).map_err(|e| format!("Invalid point: {e}"))?;
    let vk = VerifyingKey::from_encoded_point(&point)
        .map_err(|e| format!("Invalid public key: {e}"))?;

    // Parse r || s signature (big-endian, 32 + 32 bytes)
    let sig = Signature::from_slice(sig_raw).map_err(|e| format!("Invalid signature: {e}"))?;

    match vk.verify(message, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generate a known keypair for testing using p256
    use p256::ecdsa::{signature::Signer, SigningKey};
    use p256::SecretKey;

    fn test_keypair() -> (Vec<u8>, SigningKey) {
        // Deterministic key from fixed seed bytes
        let secret = SecretKey::from_slice(&[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ])
        .unwrap();
        let signing = SigningKey::from(secret);
        let verifying = signing.verifying_key();
        let point = verifying.to_encoded_point(false);
        // Skip the 0x04 prefix byte to get raw x || y
        let raw = point.as_bytes()[1..].to_vec();
        assert_eq!(raw.len(), 64);
        (raw, signing)
    }

    #[test]
    fn verify_signature_valid() {
        let (pub_key, signing_key) = test_keypair();
        let message = b"hello quantum world";
        let sig: Signature = signing_key.sign(message);
        let sig_bytes = sig.to_bytes();

        let result = verify_signature(&pub_key, message, &sig_bytes).unwrap();
        assert!(result);
    }

    #[test]
    fn verify_signature_wrong_message() {
        let (pub_key, signing_key) = test_keypair();
        let sig: Signature = signing_key.sign(b"correct message");
        let sig_bytes = sig.to_bytes();

        let result = verify_signature(&pub_key, b"wrong message", &sig_bytes).unwrap();
        assert!(!result);
    }

    #[test]
    fn verify_signature_wrong_key() {
        let (_pub_key, signing_key) = test_keypair();
        let message = b"test data";
        let sig: Signature = signing_key.sign(message);
        let sig_bytes = sig.to_bytes();

        // Different key
        let other_secret = SecretKey::from_slice(&[0xAA; 32]).unwrap();
        let other_signing = SigningKey::from(other_secret);
        let other_pub = other_signing.verifying_key().to_encoded_point(false);
        let other_raw = other_pub.as_bytes()[1..].to_vec();

        let result = verify_signature(&other_raw, message, &sig_bytes).unwrap();
        assert!(!result);
    }

    #[test]
    fn verify_certificate_valid() {
        let (ca_pub, ca_signing) = test_keypair();

        // Simulated device public key (just random-looking bytes for testing)
        let device_pub = vec![0x42; 64];
        let hw_major = 1;
        let hw_minor = 1;
        let serial_int = 217u32;

        // CA signs the certificate data
        let cert_data = build_certificate_data(hw_major, hw_minor, serial_int, &device_pub);
        let sig: Signature = ca_signing.sign(&cert_data);
        let certificate = sig.to_bytes().to_vec();

        let result = verify_certificate(
            &ca_pub,
            &device_pub,
            &certificate,
            hw_major,
            hw_minor,
            serial_int,
        )
        .unwrap();
        assert!(result);
    }

    #[test]
    fn verify_certificate_wrong_serial() {
        let (ca_pub, ca_signing) = test_keypair();

        let device_pub = vec![0x42; 64];
        let cert_data = build_certificate_data(1, 1, 217, &device_pub);
        let sig: Signature = ca_signing.sign(&cert_data);
        let certificate = sig.to_bytes().to_vec();

        // Wrong serial number
        let result =
            verify_certificate(&ca_pub, &device_pub, &certificate, 1, 1, 999).unwrap();
        assert!(!result);
    }

    #[test]
    fn verify_signature_bad_key_length() {
        let result = verify_signature(&[0u8; 32], b"msg", &[0u8; 64]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_signature_bad_sig_length() {
        let (pub_key, _) = test_keypair();
        let result = verify_signature(&pub_key, b"msg", &[0u8; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn verify_certificate_bad_lengths() {
        assert!(verify_certificate(&[0; 32], &[0; 64], &[0; 64], 1, 1, 1).is_err());
        assert!(verify_certificate(&[0; 64], &[0; 32], &[0; 64], 1, 1, 1).is_err());
        assert!(verify_certificate(&[0; 64], &[0; 64], &[0; 32], 1, 1, 1).is_err());
    }
}
