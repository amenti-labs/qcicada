//! High-level QCicada QRNG device interface.

use std::io;
use std::time::Duration;

use crate::protocol::*;
use crate::serial::{find_devices, SerialTransport};
use crate::types::*;
use crate::QCicadaError;

/// High-level interface to a QCicada QRNG device.
///
/// ```no_run
/// use qcicada::QCicada;
///
/// let mut qrng = QCicada::open(None, None)?;
/// let bytes = qrng.random(32)?;
/// println!("{:02x?}", &bytes);
/// qrng.close();
/// # Ok::<(), qcicada::QCicadaError>(())
/// ```
pub struct QCicada {
    transport: SerialTransport,
}

impl QCicada {
    /// Connect to a QCicada device.
    ///
    /// - `port`: Serial port path. If `None`, auto-discovers the first available device.
    /// - `timeout`: Default read timeout. If `None`, uses 2 seconds.
    pub fn open(port: Option<&str>, timeout: Option<Duration>) -> Result<Self, QCicadaError> {
        let timeout = timeout.unwrap_or(Duration::from_secs(2));

        let port_name = match port {
            Some(p) => p.to_string(),
            None => {
                let devices = find_devices();
                devices
                    .into_iter()
                    .next()
                    .ok_or(QCicadaError::NoDevice)?
            }
        };

        let transport = SerialTransport::open(&port_name, timeout)?;
        Ok(Self { transport })
    }

    /// Read device identification (version, serial, hardware).
    pub fn get_info(&mut self) -> Result<DeviceInfo, QCicadaError> {
        let data = self.command(CMD_GET_INFO, None)?
            .ok_or(QCicadaError::Protocol("No response to GET_INFO".into()))?;
        parse_info(&data)
    }

    /// Read current device status.
    pub fn get_status(&mut self) -> Result<DeviceStatus, QCicadaError> {
        let data = self.command(CMD_GET_STATUS, None)?
            .ok_or(QCicadaError::Protocol("No response to GET_STATUS".into()))?;
        parse_status(&data)
    }

    /// Read current device configuration.
    pub fn get_config(&mut self) -> Result<DeviceConfig, QCicadaError> {
        let data = self.command(CMD_GET_CONFIG, None)?
            .ok_or(QCicadaError::Protocol("No response to GET_CONFIG".into()))?;
        parse_config(&data)
    }

    /// Write a full device configuration.
    pub fn set_config(&mut self, config: &DeviceConfig) -> Result<(), QCicadaError> {
        let payload = serialize_config(config);
        self.command(CMD_SET_CONFIG, Some(&payload))?
            .ok_or(QCicadaError::Protocol("NACK on SET_CONFIG".into()))?;
        Ok(())
    }

    /// Read generation statistics since last reset.
    pub fn get_statistics(&mut self) -> Result<DeviceStatistics, QCicadaError> {
        let data = self.command(CMD_GET_STATISTICS, None)?
            .ok_or(QCicadaError::Protocol("No response to GET_STATISTICS".into()))?;
        parse_statistics(&data)
    }

    /// Get `n` random bytes using one-shot mode.
    ///
    /// Maximum 65535 bytes per call (protocol limit).
    pub fn random(&mut self, n: u16) -> Result<Vec<u8>, QCicadaError> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let frame = build_start_one_shot(n);
        // Send: CMD_START is frame[0], payload is frame[1..]
        self.command(CMD_START, Some(&frame[1..]))?
            .ok_or(QCicadaError::Protocol("NACK on START one-shot".into()))?;

        // Read the random data
        let timeout_ms = 500 + (n as u64) / 10;
        self.transport
            .set_timeout(Duration::from_millis(timeout_ms))?;
        let data = self.transport.read(n as usize)?;
        if data.len() != n as usize {
            return Err(QCicadaError::Protocol(format!(
                "Expected {} random bytes, got {}",
                n,
                data.len()
            )));
        }
        Ok(data)
    }

    /// Get `n` random bytes with a 64-byte cryptographic signature.
    ///
    /// Requires QCicada firmware 5.13+. The signature is produced by the device's
    /// internal asymmetric key. See device documentation for the public key.
    pub fn signed_read(&mut self, n: u16) -> Result<SignedRead, QCicadaError> {
        if n == 0 {
            return Err(QCicadaError::Protocol(
                "signed_read requires at least 1 byte".into(),
            ));
        }
        let frame = build_signed_read(n);
        self.command(CMD_SIGNED_READ, Some(&frame[1..]))?
            .ok_or(QCicadaError::Protocol("NACK on SIGNED_READ".into()))?;

        // Read random data + 64-byte signature
        let total = n as usize + SIGNATURE_LEN;
        let timeout_ms = 500 + (n as u64) / 10;
        self.transport
            .set_timeout(Duration::from_millis(timeout_ms))?;
        let buf = self.transport.read(total)?;
        if buf.len() != total {
            return Err(QCicadaError::Protocol(format!(
                "Expected {} bytes (data+sig), got {}",
                total,
                buf.len()
            )));
        }
        Ok(SignedRead {
            data: buf[..n as usize].to_vec(),
            signature: buf[n as usize..].to_vec(),
        })
    }

    /// Start continuous random data generation.
    ///
    /// After calling this, use [`read_continuous`] to read streaming data.
    /// Call [`stop`] to end continuous mode.
    pub fn start_continuous(&mut self) -> Result<(), QCicadaError> {
        let frame = build_start_continuous();
        self.command(CMD_START, Some(&frame[1..]))?
            .ok_or(QCicadaError::Protocol("NACK on START continuous".into()))?;
        Ok(())
    }

    /// Read bytes from an active continuous mode stream.
    ///
    /// Call [`start_continuous`] first. Returns exactly `n` bytes or an error.
    pub fn read_continuous(&mut self, n: usize) -> Result<Vec<u8>, QCicadaError> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let timeout_ms = 500 + (n as u64) / 10;
        self.transport
            .set_timeout(Duration::from_millis(timeout_ms))?;
        let data = self.transport.read(n)?;
        if data.len() != n {
            return Err(QCicadaError::Protocol(format!(
                "Expected {} continuous bytes, got {}",
                n,
                data.len()
            )));
        }
        Ok(data)
    }

    /// Retrieve the device's ECDSA P-256 public key (64 bytes: x || y).
    ///
    /// Requires QCicada firmware with certificate support.
    pub fn get_dev_pub_key(&mut self) -> Result<Vec<u8>, QCicadaError> {
        let data = self
            .command(CMD_GET_DEV_PUB_KEY, None)?
            .ok_or(QCicadaError::Protocol("NACK on GET_DEV_PUB_KEY".into()))?;
        if data.len() != PUB_KEY_LEN {
            return Err(QCicadaError::Protocol(format!(
                "Expected {} byte public key, got {}",
                PUB_KEY_LEN,
                data.len()
            )));
        }
        Ok(data)
    }

    /// Retrieve the device certificate (64 bytes: ECDSA r || s).
    ///
    /// This is the CA's signature over the device's identity (hw version,
    /// serial number, and public key).
    pub fn get_dev_certificate(&mut self) -> Result<Vec<u8>, QCicadaError> {
        let data = self
            .command(CMD_GET_DEV_CERTIFICATE, None)?
            .ok_or(QCicadaError::Protocol("NACK on GET_DEV_CERTIFICATE".into()))?;
        if data.len() != CERTIFICATE_LEN {
            return Err(QCicadaError::Protocol(format!(
                "Expected {} byte certificate, got {}",
                CERTIFICATE_LEN,
                data.len()
            )));
        }
        Ok(data)
    }

    /// Retrieve and verify the device's public key against a CA public key.
    ///
    /// Fetches the device's info, public key, and certificate, then verifies
    /// the certificate chain. Returns the verified public key on success.
    ///
    /// # Arguments
    /// - `ca_pub_key`: 64 bytes (x || y) of the Certificate Authority's public key.
    pub fn get_verified_pub_key(
        &mut self,
        ca_pub_key: &[u8],
    ) -> Result<Vec<u8>, QCicadaError> {
        let info = self.get_info()?;
        let dev_pub_key = self.get_dev_pub_key()?;
        let certificate = self.get_dev_certificate()?;

        let (hw_major, hw_minor) = crate::protocol::parse_hw_version(&info.hw_info)
            .ok_or_else(|| {
                QCicadaError::Protocol(format!(
                    "Cannot parse hardware version from '{}'",
                    info.hw_info
                ))
            })?;
        let serial_int = crate::protocol::parse_serial_int(&info.serial).ok_or_else(|| {
            QCicadaError::Protocol(format!(
                "Cannot parse serial number from '{}'",
                info.serial
            ))
        })?;

        let valid = crate::crypto::verify_certificate(
            ca_pub_key,
            &dev_pub_key,
            &certificate,
            hw_major,
            hw_minor,
            serial_int,
        )
        .map_err(|e| QCicadaError::Protocol(format!("Certificate verification error: {e}")))?;

        if !valid {
            return Err(QCicadaError::Protocol(
                "Device certificate verification failed".into(),
            ));
        }
        Ok(dev_pub_key)
    }

    /// Perform a signed read and verify the signature against a known device public key.
    ///
    /// Returns the verified [`SignedRead`] on success. Fails if verification fails.
    ///
    /// # Arguments
    /// - `n`: Number of random bytes to read.
    /// - `device_pub_key`: 64 bytes (x || y) of the device's verified public key.
    pub fn signed_read_verified(
        &mut self,
        n: u16,
        device_pub_key: &[u8],
    ) -> Result<SignedRead, QCicadaError> {
        let result = self.signed_read(n)?;

        let valid = crate::crypto::verify_signature(device_pub_key, &result.data, &result.signature)
            .map_err(|e| {
                QCicadaError::Protocol(format!("Signature verification error: {e}"))
            })?;

        if !valid {
            return Err(QCicadaError::Protocol(
                "Signed read signature verification failed".into(),
            ));
        }
        Ok(result)
    }

    /// Reboot the device.
    ///
    /// Sends the QCicada-specific reboot command. The device will disconnect
    /// and reconnect — you must re-open the connection after calling this.
    pub fn reboot(&mut self) -> Result<(), QCicadaError> {
        let frame = build_reboot();
        // Send the full frame (command + magic) directly
        self.transport.flush()?;
        self.transport.write(&frame)?;
        // Read optional response — device may disconnect immediately
        self.transport.set_timeout(Duration::from_millis(500))?;
        let _ = self.transport.read(1);
        Ok(())
    }

    /// Change post-processing mode, preserving other config settings.
    pub fn set_postprocess(&mut self, mode: PostProcess) -> Result<(), QCicadaError> {
        let mut config = self.get_config()?;
        config.postprocess = mode;
        self.set_config(&config)
    }

    /// Reset the device (restarts startup test and clears statistics).
    pub fn reset(&mut self) -> Result<(), QCicadaError> {
        self.command(CMD_RESET, None)?
            .ok_or(QCicadaError::Protocol("NACK on RESET".into()))?;
        Ok(())
    }

    /// Send STOP command to halt any active generation.
    pub fn stop(&mut self) -> Result<(), QCicadaError> {
        self.command(CMD_STOP, None)?;
        Ok(())
    }

    /// Fill a buffer with random bytes, chunking as needed for the protocol limit.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) -> Result<(), QCicadaError> {
        let mut offset = 0;
        while offset < buf.len() {
            let chunk = (buf.len() - offset).min(u16::MAX as usize) as u16;
            let data = self.random(chunk)?;
            buf[offset..offset + data.len()].copy_from_slice(&data);
            offset += data.len();
        }
        Ok(())
    }

    /// Close the serial connection.
    pub fn close(self) {
        drop(self);
    }

    // --- Internal protocol handling ---

    /// Send a command and read the response.
    ///
    /// Returns `Ok(Some(payload))` on success with payload,
    /// `Ok(Some(empty vec))` on success without payload (ACK with no data expected beyond status),
    /// or `Ok(None)` on NACK.
    fn command(
        &mut self,
        cmd_code: u8,
        payload: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, QCicadaError> {
        let expected = expected_response(cmd_code)
            .ok_or_else(|| QCicadaError::Protocol(format!("Unknown command: {cmd_code:#04x}")))?;

        self.transport.flush()?;

        let frame = build_cmd(cmd_code, payload);
        self.transport.write(&frame)?;

        // STOP command: drain buffer, find ACK near end
        if cmd_code == CMD_STOP {
            return self.handle_stop();
        }

        // Read 1-byte response code
        self.transport.set_timeout(Duration::from_secs(3))?;
        let resp = self.transport.read(1)?;
        if resp.is_empty() {
            return Ok(None);
        }

        if resp[0] == expected {
            let size = payload_size(expected);
            if size == 0 {
                return Ok(Some(Vec::new()));
            }
            let timeout_ms = (size as u64).max(500);
            self.transport
                .set_timeout(Duration::from_millis(timeout_ms))?;
            let resp_payload = self.transport.read(size)?;
            if resp_payload.len() != size {
                return Ok(None);
            }
            Ok(Some(resp_payload))
        } else if resp[0] == RESP_NACK {
            Ok(None)
        } else {
            Err(QCicadaError::Protocol(format!(
                "Unexpected response byte: {:#04x}",
                resp[0]
            )))
        }
    }

    /// Handle STOP command response: drain pipe and find ACK.
    fn handle_stop(&mut self) -> Result<Option<Vec<u8>>, QCicadaError> {
        let drain_size = MAX_BLOCK_SIZE * 2 + PAYLOAD_ACK + 1;

        self.transport.set_timeout(Duration::from_millis(500))?;

        for _ in 0..2 {
            let resp = self.transport.read(drain_size)?;
            if resp.len() == 1 && resp[0] == RESP_NACK {
                return Ok(None);
            }
            if resp.len() < PAYLOAD_ACK + 1 {
                return Ok(None);
            }
            let ack_pos = resp.len() - 1 - PAYLOAD_ACK;
            if resp[ack_pos] == RESP_ACK {
                return Ok(Some(Vec::new()));
            }
        }
        Ok(None)
    }
}

impl io::Read for QCicada {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = buf.len().min(u16::MAX as usize) as u16;
        let data = self
            .random(n)
            .map_err(io::Error::other)?;
        buf[..data.len()].copy_from_slice(&data);
        Ok(data.len())
    }
}
