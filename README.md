# qcicada

Rust and Python SDK for the [QCicada quantum random number generator](https://cryptalabs.com/) by Crypta Labs.

macOS-first — fixes FTDI serial driver issues that break the official SDK. Works on Linux too.

## Install

**Python**
```bash
pip install qcicada
```

**Rust**
```toml
[dependencies]
qcicada = "0.1"
```

## Quick Start

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
from qcicada import QCicada

with QCicada() as qrng:
    print(qrng.random(32).hex())
```

</td><td>

```rust
use qcicada::QCicada;

let mut qrng = QCicada::open(None, None)?;
let bytes = qrng.random(32)?;
println!("{:02x?}", bytes);
```

</td></tr>
</table>

The device is auto-detected. If you have multiple USB-serial devices, pass the port explicitly:

```python
QCicada(port="/dev/cu.usbserial-DK0HFP4T")      # Python
```
```rust
QCicada::open(Some("/dev/cu.usbserial-DK0HFP4T"), None)?;  // Rust
```

## Device Discovery

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
from qcicada import (
    find_devices,
    discover_devices,
    open_by_serial,
)

# Fast port scan (no device I/O)
find_devices()
# ['/dev/cu.usbserial-DK0HFP4T']

# Probe and verify each device
for dev in discover_devices():
    print(dev.port, dev.info.serial)

# Open by serial number
qrng = open_by_serial("QC0000000217")
```

</td><td>

```rust
use qcicada::{
    find_devices,
    discover_devices,
    open_by_serial,
};

// Fast port scan (no device I/O)
let ports = find_devices();

// Probe and verify each device
for dev in discover_devices() {
    println!("{} {}", dev.port, dev.info.serial);
}

// Open by serial number
let mut qrng = open_by_serial("QC0000000217")?;
```

</td></tr>
</table>

## Entropy Modes

The QCicada supports three post-processing modes:

| Mode | What you get |
|------|-------------|
| **SHA256** (default) | NIST SP 800-90B conditioned output — use for cryptography |
| **Raw Noise** | After health-test conditioning — use for entropy research |
| **Raw Samples** | Unprocessed samples from the quantum optical module |

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
from qcicada import QCicada, PostProcess

with QCicada() as qrng:
    qrng.random(32)  # SHA256 (default)

    qrng.set_postprocess(PostProcess.RAW_NOISE)
    qrng.random(32)

    qrng.set_postprocess(PostProcess.RAW_SAMPLES)
    qrng.random(32)
```

</td><td>

```rust
use qcicada::{QCicada, PostProcess};

let mut qrng = QCicada::open(None, None)?;
qrng.random(32)?;  // SHA256 (default)

qrng.set_postprocess(PostProcess::RawNoise)?;
qrng.random(32)?;

qrng.set_postprocess(PostProcess::RawSamples)?;
qrng.random(32)?;
```

</td></tr>
</table>

## Signed Reads

Random bytes with a 64-byte cryptographic signature from the device's internal key. Requires firmware 5.13+.

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
result = qrng.signed_read(32)
result.data       # 32 random bytes
result.signature  # 64-byte signature
```

</td><td>

```rust
let result = qrng.signed_read(32)?;
result.data       // 32 random bytes
result.signature  // 64-byte signature
```

</td></tr>
</table>

## Certificate Verification

Verify the device's identity using its ECDSA P-256 certificate chain. The device holds an internal keypair; a Certificate Authority (CA) signs the device's public key along with its hardware version and serial number.

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
# CA public key (64 bytes, from Crypta Labs)
ca_pub_key = bytes.fromhex("...")

# Verify device and get its public key
dev_pub = qrng.get_verified_pub_key(ca_pub_key)

# Signed read with signature verification
result = qrng.signed_read_verified(32, dev_pub)
result.data       # 32 verified random bytes
result.signature  # 64-byte signature
```

</td><td>

```rust
// CA public key (64 bytes, from Crypta Labs)
let ca_pub_key = hex::decode("...").unwrap();

// Verify device and get its public key
let dev_pub = qrng.get_verified_pub_key(&ca_pub_key)?;

// Signed read with signature verification
let result = qrng.signed_read_verified(32, &dev_pub)?;
result.data       // 32 verified random bytes
result.signature  // 64-byte signature
```

</td></tr>
</table>

You can also access the raw primitives directly:

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
pub_key = qrng.get_dev_pub_key()      # 64 bytes
cert = qrng.get_dev_certificate()     # 64 bytes

from qcicada import verify_certificate
valid = verify_certificate(
    ca_pub_key, pub_key, cert,
    hw_major=1, hw_minor=1, serial_int=217,
)
```

</td><td>

```rust
let pub_key = qrng.get_dev_pub_key()?;   // 64 bytes
let cert = qrng.get_dev_certificate()?;  // 64 bytes

use qcicada::crypto::verify_certificate;
let valid = verify_certificate(
    &ca_pub_key, &pub_key, &cert, 1, 1, 217,
)?;
```

</td></tr>
</table>

## Continuous Mode

High-throughput streaming with no per-request overhead:

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
qrng.start_continuous()
for _ in range(100):
    chunk = qrng.read_continuous(1024)
qrng.stop()
```

</td><td>

```rust
qrng.start_continuous()?;
for _ in 0..100 {
    let chunk = qrng.read_continuous(1024)?;
}
qrng.stop()?;
```

</td></tr>
</table>

## Device Info & Status

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
info = qrng.get_info()
# serial, fw_version, core_version, hw_info

status = qrng.get_status()
# initialized, ready_bytes, health flags...

stats = qrng.get_statistics()
# generated_bytes, speed, failure counts...

config = qrng.get_config()
# postprocess, block_size, auto_calibration...
```

</td><td>

```rust
let info = qrng.get_info()?;
// serial, fw_version, core_version, hw_info

let status = qrng.get_status()?;
// initialized, ready_bytes, health flags...

let stats = qrng.get_statistics()?;
// generated_bytes, speed, failure counts...

let config = qrng.get_config()?;
// postprocess, block_size, auto_calibration...
```

</td></tr>
</table>

## Configuration

Every device setting is readable and writable:

<table>
<tr><th>Python</th><th>Rust</th></tr>
<tr><td>

```python
from dataclasses import replace

config = qrng.get_config()
config = replace(config,
    block_size=256,
    auto_calibration=False,
)
qrng.set_config(config)
```

</td><td>

```rust
let mut config = qrng.get_config()?;
config.block_size = 256;
config.auto_calibration = false;
qrng.set_config(&config)?;
```

</td></tr>
</table>

| Field | Type | Description |
|-------|------|-------------|
| `postprocess` | `PostProcess` | SHA256, RawNoise, or RawSamples |
| `initial_level` | `f32` | LED initial level |
| `startup_test` | `bool` | Run health test on startup |
| `auto_calibration` | `bool` | Auto-calibrate light source |
| `repetition_count` | `bool` | NIST SP 800-90B repetition count test |
| `adaptive_proportion` | `bool` | NIST SP 800-90B adaptive proportion test |
| `bit_count` | `bool` | Crypta Labs bit balance test |
| `generate_on_error` | `bool` | Keep generating if a health test fails |
| `n_lsbits` | `u8` | Number of LSBs to extract per sample |
| `hash_input_size` | `u8` | Bytes fed into SHA256 per output block |
| `block_size` | `u16` | Output block size in bytes |
| `autocalibration_target` | `u16` | Target value for auto-calibration |

## API Reference

| Method | Description |
|--------|-------------|
| `random(n)` | Get `n` random bytes (1–65535, one-shot) |
| `signed_read(n)` | Get `n` random bytes + 64-byte signature (FW 5.13+) |
| `signed_read_verified(n, pub_key)` | Signed read + ECDSA signature verification |
| `start_continuous()` | Start continuous streaming mode |
| `read_continuous(n)` | Read `n` bytes from continuous stream |
| `fill_bytes(buf)` | Fill a buffer of any size (auto-chunks) |
| `get_info()` | Serial number, firmware version, hardware |
| `get_status()` | Health flags, ready byte count |
| `get_config()` | Full device configuration |
| `set_config(config)` | Write device configuration |
| `set_postprocess(mode)` | Shortcut to change entropy mode |
| `get_statistics()` | Bytes generated, speed, failure counts |
| `get_dev_pub_key()` | Device's ECDSA P-256 public key (64 bytes) |
| `get_dev_certificate()` | CA-signed device certificate (64 bytes) |
| `get_verified_pub_key(ca_key)` | Verify certificate chain, return device public key |
| `reboot()` | Reboot the device (reconnect required) |
| `reset()` | Restart generation and clear statistics |
| `stop()` | Halt any active generation |
| `close()` | Close serial port |

Rust also implements `std::io::Read`, so `QCicada` works anywhere a reader is expected.

## Project Structure

```
qcicada/
├── src/              # Rust crate
│   ├── lib.rs
│   ├── device.rs     # QCicada high-level API
│   ├── protocol.rs   # Wire protocol (pure, no I/O)
│   ├── crypto.rs     # ECDSA P-256 certificate verification
│   ├── serial.rs     # Serial transport + macOS fixes
│   ├── discovery.rs  # Device discovery
│   └── types.rs      # Shared data types
├── tests/            # Rust integration tests (device required)
├── examples/         # Rust examples
├── python/
│   ├── src/qcicada/  # Python package (mirrors Rust API)
│   ├── tests/        # Python unit + integration tests
│   └── examples/     # Python examples
├── Cargo.toml
└── python/pyproject.toml
```

Both SDKs implement the same wire protocol and share the same test vectors. Changes to one should be reflected in the other.

## Why Not pyqcc?

The official Crypta Labs SDK (`pyqcc`) has macOS issues:

- Uses `/dev/tty.*` ports — macOS needs `/dev/cu.*`
- Sets `inter_byte_timeout` — causes FTDI read failures on macOS
- Timeouts too short — macOS FTDI driver needs at least 500ms
- No flush delay — FTDI driver drops bytes without a post-write pause
- Device may be left in continuous mode — no drain on connect

This SDK fixes all of these. It also works fine on Linux.

## License

MIT
