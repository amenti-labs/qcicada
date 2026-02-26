# qcicada

Rust and Python SDK for the [QCicada quantum random number generator](https://cryptalabs.com/) by Crypta Labs.

The official `pyqcc` SDK doesn't work on macOS due to FTDI serial driver differences. This SDK fixes that — and provides a cleaner API with no monkey-patching.

## Quick Start

### Python

```bash
pip install ./python    # requires pyserial
```

```python
from qcicada import QCicada

with QCicada() as qrng:
    print(qrng.random(32).hex())
```

### Rust

```toml
[dependencies]
qcicada = "0.1"
```

```rust
use qcicada::QCicada;

let mut qrng = QCicada::open(None, None)?;
let bytes = qrng.random(32)?;
```

That's it. The device is auto-detected. If you have multiple USB-serial devices, pass the port explicitly: `QCicada(port="/dev/cu.usbserial-DK0HFP4T")`.

## Finding Your Device

```python
from qcicada import find_devices, discover_devices, open_by_serial

# List USB-serial ports (fast, no device communication)
find_devices()
# ['/dev/cu.usbserial-DK0HFP4T']

# Probe each port and confirm it's actually a QCicada
for dev in discover_devices():
    print(f"{dev.port}  serial={dev.info.serial}  hw={dev.info.hw_info}")
# /dev/cu.usbserial-DK0HFP4T  serial=QC0000000217  hw=CICADA-QRNG-1.1

# Open a specific device by serial number
qrng = open_by_serial("QC0000000217")
```

The same functions are available in Rust.

## Entropy Modes

The QCicada supports three post-processing modes:

| Mode | What you get |
|------|-------------|
| **SHA256** (default) | NIST SP 800-90B conditioned output — use this for cryptography |
| **Raw Noise** | Noise after health-test conditioning — use this for entropy research |
| **Raw Samples** | Unprocessed samples from the quantum optical module |

```python
from qcicada import QCicada, PostProcess

with QCicada() as qrng:
    # Default: SHA256
    qrng.random(32).hex()

    # Switch to raw noise
    qrng.set_postprocess(PostProcess.RAW_NOISE)
    qrng.random(32).hex()

    # Switch to raw samples
    qrng.set_postprocess(PostProcess.RAW_SAMPLES)
    qrng.random(32).hex()
```

## Device Info & Status

```python
with QCicada() as qrng:
    info = qrng.get_info()
    # DeviceInfo(serial='QC0000000217', fw_version=0x5000e,
    #            core_version=0x1000c, hw_info='CICADA-QRNG-1.1')

    status = qrng.get_status()
    # DeviceStatus(initialized=True, ready_bytes=13440, ...)

    stats = qrng.get_statistics()
    # DeviceStatistics(generated_bytes=4928, speed=100696, ...)

    config = qrng.get_config()
    # DeviceConfig(postprocess=SHA256, auto_calibration=True, block_size=448, ...)
```

## Full Configuration

Every device setting is readable and writable:

```python
from dataclasses import replace

config = qrng.get_config()

# Modify and write back
config = replace(config, block_size=256, auto_calibration=False)
qrng.set_config(config)
```

| Field | Type | Description |
|-------|------|-------------|
| `postprocess` | `PostProcess` | SHA256, RawNoise, or RawSamples |
| `initial_level` | `float` | LED initial level |
| `startup_test` | `bool` | Run health test on startup |
| `auto_calibration` | `bool` | Auto-calibrate light source |
| `repetition_count` | `bool` | NIST SP 800-90B repetition count test |
| `adaptive_proportion` | `bool` | NIST SP 800-90B adaptive proportion test |
| `bit_count` | `bool` | Crypta Labs bit balance test |
| `generate_on_error` | `bool` | Keep generating if a health test fails |
| `n_lsbits` | `int` | Number of LSBs to extract per sample |
| `hash_input_size` | `int` | Bytes fed into SHA256 per output block |
| `block_size` | `int` | Output block size in bytes |
| `autocalibration_target` | `int` | Target value for auto-calibration |

## API Reference

| Method | Description |
|--------|-------------|
| `random(n)` | Get `n` random bytes (1–65535, one-shot) |
| `fill_bytes(buf)` | Fill a buffer of any size (auto-chunks) |
| `get_info()` | Serial number, firmware version, hardware |
| `get_status()` | Health flags, ready byte count |
| `get_config()` | Full device configuration |
| `set_config(config)` | Write device configuration |
| `set_postprocess(mode)` | Shortcut to change entropy mode |
| `get_statistics()` | Bytes generated, speed, failure counts |
| `reset()` | Restart generation and clear statistics |
| `stop()` | Halt any active generation |
| `close()` | Close serial port |

Rust: `QCicada` also implements `std::io::Read`, so it works anywhere a reader is expected.

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
