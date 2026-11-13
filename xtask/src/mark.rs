//! Encode the supplied SVG without changing its bytes. The Site profile refuses SVG
//! asset files; its image CSP permits data URIs. Run `cargo xtask mark` to regenerate
//! the template, or `cargo xtask mark --check` to check it. Never edit the SVG to
//! change its appearance: replace the approved source instead.

use std::fs;
use std::path::Path;

pub(crate) fn command(root: &Path, check: bool) -> Result<bool, String> {
    let source = root.join("crates/fieldbook-render/assets/fieldbook.svg");
    let output = root.join("crates/fieldbook-render/templates/frame/mark-uri.txt");
    let bytes = fs::read(&source).map_err(|error| error.to_string())?;
    let mut uri = String::from("data:image/svg+xml,");
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in bytes {
        uri.push('%');
        uri.push(char::from(HEX[usize::from(byte >> 4)]));
        uri.push(char::from(HEX[usize::from(byte & 15)]));
    }
    if check {
        if fs::read(&output).is_ok_and(|actual| actual == uri.as_bytes()) {
            println!("Fieldbook mark URI matches the supplied SVG bytes");
            Ok(true)
        } else {
            eprintln!("Fieldbook mark URI differs; run: cargo xtask mark");
            Ok(false)
        }
    } else {
        fs::write(&output, uri).map_err(|error| error.to_string())?;
        Ok(true)
    }
}
