#!/usr/bin/env python3
"""Quick-start example for the qcicada SDK."""

from qcicada import QCicada, PostProcess, discover_devices

# Discover connected devices
devices = discover_devices()
if not devices:
    print("No QCicada devices found.")
    raise SystemExit(1)

for dev in devices:
    print(f"Found: {dev.port} — {dev.info.serial} ({dev.info.hw_info})")

with QCicada() as qrng:
    # Device info
    info = qrng.get_info()
    print(f"\nSerial: {info.serial}")
    print(f"FW:     {info.fw_version:#06x}")
    print(f"Core:   {info.core_version:#06x}")
    print(f"HW:     {info.hw_info}")

    # Status
    status = qrng.get_status()
    print(f"\nReady bytes: {status.ready_bytes}")
    print(f"Initialized: {status.initialized}")

    # Current config
    config = qrng.get_config()
    print(f"\nPost-processing: {config.postprocess.name}")
    print(f"Auto-calibration: {config.auto_calibration}")
    print(f"Block size: {config.block_size}")

    # SHA256 mode (default, NIST compliant)
    print(f"\nSHA256:      {qrng.random(32).hex()}")

    # Raw noise mode
    qrng.set_postprocess(PostProcess.RAW_NOISE)
    print(f"Raw Noise:   {qrng.random(32).hex()}")

    # Raw samples mode
    qrng.set_postprocess(PostProcess.RAW_SAMPLES)
    print(f"Raw Samples: {qrng.random(32).hex()}")

    # Restore default
    qrng.set_postprocess(PostProcess.SHA256)
    print("\nRestored SHA256 mode.")

    # Statistics
    stats = qrng.get_statistics()
    print(f"\nGenerated: {stats.generated_bytes} bytes")
    print(f"Speed: {stats.speed} bits/s")
