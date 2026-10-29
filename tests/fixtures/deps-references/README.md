# Synthetic reference inputs

All inputs are public, synthetic and immutable. `expected.json` files and `errors.json` are independently authored literal oracles, never produced by the reader. Each case is a complete root; negative cases deliberately replace or remove one input.

`bytes.bin` is 200 fixed 32-byte strings, generated once with Python `random.Random(17)` with the first 100 over all byte values and the remaining 100 over the ASCII alphabet `abc uses:#"'{}[]|>!&*-_0123456789`, newline and space. It is data, not a test generator. The counted Rust test copies a complete root, perturbs each input separately, and restores the other input.
