use qcicada::{discover_devices, PostProcess, QCicada};

fn main() -> Result<(), qcicada::QCicadaError> {
    // Discover connected devices
    let devices = discover_devices();
    if devices.is_empty() {
        eprintln!("No QCicada devices found.");
        std::process::exit(1);
    }
    for dev in &devices {
        println!("Found: {} — {} ({})", dev.port, dev.info.serial, dev.info.hw_info);
    }

    let mut qrng = QCicada::open(None, None)?;

    // Device info
    let info = qrng.get_info()?;
    println!("\nSerial: {}", info.serial);
    println!("FW:     {:#06x}", info.fw_version);
    println!("Core:   {:#06x}", info.core_version);
    println!("HW:     {}", info.hw_info);

    // Status
    let status = qrng.get_status()?;
    println!("\nReady bytes: {}", status.ready_bytes);
    println!("Initialized: {}", status.initialized);

    // Current config
    let config = qrng.get_config()?;
    println!("\nPost-processing: {:?}", config.postprocess);
    println!("Auto-calibration: {}", config.auto_calibration);
    println!("Block size: {}", config.block_size);

    // SHA256 mode (default, NIST compliant)
    let bytes = qrng.random(32)?;
    println!("\nSHA256:      {}", hex::encode(&bytes));

    // Raw noise mode
    qrng.set_postprocess(PostProcess::RawNoise)?;
    let bytes = qrng.random(32)?;
    println!("Raw Noise:   {}", hex::encode(&bytes));

    // Raw samples mode
    qrng.set_postprocess(PostProcess::RawSamples)?;
    let bytes = qrng.random(32)?;
    println!("Raw Samples: {}", hex::encode(&bytes));

    // Restore default
    qrng.set_postprocess(PostProcess::Sha256)?;
    println!("\nRestored SHA256 mode.");

    // Statistics
    let stats = qrng.get_statistics()?;
    println!("\nGenerated: {} bytes", stats.generated_bytes);
    println!("Speed: {} bits/s", stats.speed);

    Ok(())
}
