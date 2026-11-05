## Adding a case

1. Copy a case file; give it the next `CALC-nn` id (ids 90 to 99 are for refusal cases), the provenance fields (`synthetic = true`, `as_of`, `overhead_base`, `stages`), and the `fn` it exercises.
2. State the inputs and expected outputs as decimal text, or, for a negative case, `refuse = "<code>"`.
3. Add the file to `CASES` in `crates/fieldbook-core/src/cases.rs`.
4. Increase `CALC_EPOCH` when the case-set binding differs from the selected baseline, then write the current digest into `EPOCH` and update `compat/published.toml`.
5. Run `cargo nextest run --locked -p fieldbook-calc cases::` and `cargo nextest run --locked -p fieldbook-core --test calc_epoch`.
