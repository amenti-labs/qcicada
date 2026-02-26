//! Platform-aware serial transport for QCicada QRNG devices.

use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::Duration;

use crate::QCicadaError;

/// Auto-discover QCicada QRNG devices by serial port pattern.
pub fn find_devices() -> Vec<String> {
    let pattern = if cfg!(target_os = "macos") {
        "/dev/cu.usbserial-"
    } else {
        "/dev/ttyUSB"
    };

    let mut devices: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .filter(|name| name.starts_with(pattern))
        .collect();
    devices.sort();
    devices
}

/// Serial transport with macOS FTDI workarounds.
///
/// On macOS:
/// - Minimum 500ms read timeout (FTDI driver needs it)
/// - Flush + 50ms delay after every write
///
/// On Linux:
/// - Standard serial behavior
pub struct SerialTransport {
    port: Box<dyn SerialPort>,
    is_macos: bool,
}

const MIN_TIMEOUT_MACOS: Duration = Duration::from_millis(500);

impl SerialTransport {
    /// Open a serial connection to the given port.
    pub fn open(port_name: &str, timeout: Duration) -> Result<Self, QCicadaError> {
        let is_macos = cfg!(target_os = "macos");

        let mut port = serialport::new(port_name, 1_000_000)
            .timeout(timeout)
            .open()
            .map_err(|e| QCicadaError::Serial(format!("Failed to open {port_name}: {e}")))?;

        // Stop any continuous mode and drain the buffer
        port.write_all(&[0x05])
            .map_err(|e| QCicadaError::Serial(format!("Init write failed: {e}")))?;
        std::thread::sleep(Duration::from_millis(500));

        port.set_timeout(Duration::from_millis(300))
            .map_err(|e| QCicadaError::Serial(format!("Set timeout failed: {e}")))?;

        let mut drain = [0u8; 4096];
        loop {
            match port.read(&mut drain) {
                Ok(0) => break,
                Err(_) => break,
                Ok(_) => continue,
            }
        }

        port.clear(serialport::ClearBuffer::Input)
            .map_err(|e| QCicadaError::Serial(format!("Clear buffer failed: {e}")))?;

        port.set_timeout(timeout)
            .map_err(|e| QCicadaError::Serial(format!("Set timeout failed: {e}")))?;

        std::thread::sleep(Duration::from_millis(100));

        Ok(Self { port, is_macos })
    }

    /// Write data. On macOS, flushes and waits for FTDI latency.
    pub fn write(&mut self, data: &[u8]) -> Result<(), QCicadaError> {
        self.port
            .write_all(data)
            .map_err(|e| QCicadaError::Serial(format!("Write failed: {e}")))?;
        if self.is_macos {
            self.port
                .flush()
                .map_err(|e| QCicadaError::Serial(format!("Flush failed: {e}")))?;
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    /// Read exactly `len` bytes (returns fewer on timeout).
    pub fn read(&mut self, len: usize) -> Result<Vec<u8>, QCicadaError> {
        let mut buf = vec![0u8; len];
        let mut total = 0;
        while total < len {
            match self.port.read(&mut buf[total..]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => return Err(QCicadaError::Serial(format!("Read failed: {e}"))),
            }
        }
        buf.truncate(total);
        Ok(buf)
    }

    /// Flush output and clear input buffer.
    pub fn flush(&mut self) -> Result<(), QCicadaError> {
        self.port
            .flush()
            .map_err(|e| QCicadaError::Serial(format!("Flush failed: {e}")))?;
        self.port
            .clear(serialport::ClearBuffer::Input)
            .map_err(|e| QCicadaError::Serial(format!("Clear buffer failed: {e}")))?;
        Ok(())
    }

    /// Set read timeout, enforcing macOS minimum.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), QCicadaError> {
        let timeout = if self.is_macos {
            timeout.max(MIN_TIMEOUT_MACOS)
        } else {
            timeout
        };
        self.port
            .set_timeout(timeout)
            .map_err(|e| QCicadaError::Serial(format!("Set timeout failed: {e}")))?;
        Ok(())
    }
}
