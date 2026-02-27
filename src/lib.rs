//! # qcicada
//!
//! Rust SDK for the QCicada QRNG (Crypta Labs).
//!
//! macOS-first — fixes FTDI serial driver issues that prevent standard
//! serial communication. Also works on Linux.
//!
//! ## Quick Start
//!
//! ```no_run
//! use qcicada::{QCicada, PostProcess};
//!
//! let mut qrng = QCicada::open(None, None)?;
//!
//! let info = qrng.get_info()?;
//! println!("Serial: {}, FW: {:#06x}", info.serial, info.fw_version);
//!
//! // SHA256 mode (default, NIST compliant)
//! let bytes = qrng.random(32)?;
//! println!("{:02x?}", &bytes);
//!
//! // Raw noise mode
//! qrng.set_postprocess(PostProcess::RawNoise)?;
//! let bytes = qrng.random(32)?;
//! println!("{:02x?}", &bytes);
//! # Ok::<(), qcicada::QCicadaError>(())
//! ```

pub mod crypto;
pub mod device;
pub mod discovery;
pub mod protocol;
pub mod serial;
pub mod types;

pub use device::QCicada;
pub use discovery::{discover_devices, open_by_serial, probe_device, DiscoveredDevice};
pub use serial::find_devices;
pub use types::*;

/// Errors returned by the qcicada SDK.
#[derive(Debug, thiserror::Error)]
pub enum QCicadaError {
    /// No QCicada device found during auto-discovery.
    #[error("No QCicada device found")]
    NoDevice,

    /// Serial communication error.
    #[error("Serial error: {0}")]
    Serial(String),

    /// Protocol-level error (unexpected response, parse failure).
    #[error("Protocol error: {0}")]
    Protocol(String),
}
