# Synthetic configuration inputs

All stored cases are public, synthetic and immutable. `Cargo.toml.input` stores the exact file bytes or deliberate directory kind; counted Rust setup copies each case into an owned temporary root and restores the canonical `Cargo.toml` name. Absent payloads remain absent. JSON oracles are literal expected facts/errors, authored independently of the reader. Replay records deliberately include invalid semantic values; the producer only reads their supported shapes.

`bytes.bin` reuses the published reference fixture's 200 fixed 32-byte samples (seed 17). Counted Rust code perturbs each configuration file separately while all other inputs remain valid, then exercises every single-byte mutation of the baseline. The CRLF case uses fixture-local Git attributes so its line endings survive checkout.
