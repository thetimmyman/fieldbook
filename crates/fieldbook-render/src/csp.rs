//! Content-Security-Policy constants for static pages (`CSP_STATIC`) and the runtime chat bridge (`CSP_BRIDGE`).

/// Content-Security-Policy for static pages written to a Corpus or an edition (`connect-src 'none'`).
pub const CSP_STATIC: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; form-action 'none'; base-uri 'none'";

/// Content-Security-Policy for the runtime chat bridge page (`connect-src 'self'`).
pub const CSP_BRIDGE: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; form-action 'none'; base-uri 'none'";
