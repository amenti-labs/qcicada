# Changelog

## 0.2.1

### Fixed

- **Chunked serial reads for large requests** — `random()`, `read_continuous()`,
  and `signed_read()` now read the serial response in 8192-byte chunks with
  per-chunk adaptive timeouts, matching the official QCC C library's
  `_read_random()` pattern. Previously, a single `transport.read(n)` call for
  large `n` could exceed the USB serial timeout and fail.

- **`fill_bytes()` chunk size** — reduced from `u16::MAX` (65535) to 8192 bytes
  per one-shot protocol command. Each `random()` call carries protocol overhead
  (START command + ACK), so smaller chunks stay within reliable serial timeout
  limits while amortizing that overhead.

- **`io::Read` impl** — capped per-call size at 8192 bytes (was `u16::MAX`) to
  prevent timeout failures when callers pass large buffers.

## 0.2.0

- Certificate verification (ECDSA P-256)
- Separated unit tests from device integration tests
- Integration tests for certificate verification on device

## 0.1.1

- Fix repository URL
- Add PyPI metadata and readme symlink for Python package

## 0.1.0

- Initial release: one-shot reads, signed reads, continuous mode
- macOS FTDI workarounds, device auto-discovery
