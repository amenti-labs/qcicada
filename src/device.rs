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
