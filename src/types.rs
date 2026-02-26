/// Post-processing mode for random data output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PostProcess {
    /// NIST SP 800-90B compliant SHA256 conditioning (default).
    Sha256 = 0,
    /// Raw noise after health-test conditioning.
    RawNoise = 1,
    /// Raw samples directly from QOM, no conditioning.
    RawSamples = 2,
}

impl TryFrom<u8> for PostProcess {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Self::Sha256),
            1 => Ok(Self::RawNoise),
            2 => Ok(Self::RawSamples),
            _ => Err(v),
        }
    }
}

/// Device identification and version information.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub core_version: u32,
    pub fw_version: u32,
    pub serial: String,
    pub hw_info: String,
}

/// Current device operational status.
#[derive(Debug, Clone)]
pub struct DeviceStatus {
    pub initialized: bool,
    pub startup_test_in_progress: bool,
    pub voltage_low: bool,
    pub voltage_high: bool,
    pub voltage_undefined: bool,
    pub bitcount: bool,
    pub repetition_count: bool,
    pub adaptive_proportion: bool,
    pub ready_bytes: u32,
}

/// Device configuration (full mode).
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub postprocess: PostProcess,
    pub initial_level: f32,
    pub startup_test: bool,
    pub auto_calibration: bool,
    pub repetition_count: bool,
    pub adaptive_proportion: bool,
    pub bit_count: bool,
    pub generate_on_error: bool,
    pub n_lsbits: u8,
    pub hash_input_size: u8,
    pub block_size: u16,
    pub autocalibration_target: u16,
}

/// Device generation statistics since last reset.
#[derive(Debug, Clone)]
pub struct DeviceStatistics {
    pub generated_bytes: u64,
    pub repetition_count_failures: u32,
    pub adaptive_proportion_failures: u32,
    pub bitcount_failures: u32,
    pub speed: u32,
    pub sensif_average: u16,
    pub ledctrl_level: f32,
}

/// Result of a signed read: random data + cryptographic signature.
#[derive(Debug, Clone)]
pub struct SignedRead {
    /// The random bytes.
    pub data: Vec<u8>,
    /// 64-byte cryptographic signature over the data.
    pub signature: Vec<u8>,
}
