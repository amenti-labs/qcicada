//! Device discovery: find and identify QCicada QRNG devices.

use std::time::Duration;

use crate::device::QCicada;
use crate::serial::find_devices;
use crate::types::DeviceInfo;
use crate::QCicadaError;

/// A discovered QCicada device with its port and identification.
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    /// Serial port path (e.g. `/dev/cu.usbserial-DK0HFP4T`).
    pub port: String,
    /// Device info (version, serial number, hardware).
    pub info: DeviceInfo,
}

/// Probe a specific port to check if a QCicada device is connected.
///
/// Opens the port, sends GET_INFO, and returns the device info if it responds
/// correctly. Returns `None` if the port is not a QCicada (or is unavailable).
pub fn probe_device(port: &str) -> Option<DeviceInfo> {
    let mut dev = QCicada::open(Some(port), Some(Duration::from_secs(2))).ok()?;
    dev.get_info().ok()
}

/// Discover all connected QCicada devices.
///
/// Scans matching serial ports and probes each one with GET_INFO to verify
/// it's actually a QCicada. Returns only confirmed devices with their info.
///
/// This is slower than `find_devices()` (which only checks port names) but
/// guarantees the returned ports are real QCicada devices.
pub fn discover_devices() -> Vec<DiscoveredDevice> {
    find_devices()
        .into_iter()
        .filter_map(|port| {
            let info = probe_device(&port)?;
            Some(DiscoveredDevice { port, info })
        })
        .collect()
}

/// Open a QCicada device by its serial number.
///
/// Probes all matching ports and opens the one whose serial number matches.
pub fn open_by_serial(serial: &str) -> Result<QCicada, QCicadaError> {
    for port in find_devices() {
        if let Some(info) = probe_device(&port) {
            if info.serial == serial {
                return QCicada::open(Some(&port), None);
            }
        }
    }
    Err(QCicadaError::NoDevice)
}
