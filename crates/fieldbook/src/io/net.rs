//! The non-socket half of outbound HTTP (`model-gate` step 3, `ADR 0004 §5`).
//!
//! This file holds no socket type. `net-stack-spike` adds `UreqHttp` and its crates behind the
//! `net` feature; until then only the scripted and panicking doubles exist. Every send takes the
//! `Allowed` token the gate issued, so a transport cannot send what the gate did not clear.

use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use fieldbook_core::egress::{Allowed, Operation, Target};

use crate::io::keys::Key;

/// Default cap on a model reply body (4 MiB).
pub const DEFAULT_MAX_REPLY_BYTES: u64 = 4 * 1024 * 1024;

/// One header on an outbound request.
pub enum Header {
    /// A header whose value is not a secret.
    Plain {
        /// Header name.
        name: String,
        /// Header value.
        value: String,
    },
    /// A secret header that holds a [`Key`] and is shown as its name only.
    Secret {
        /// Header name.
        name: String,
        /// Secret value.
        key: Key,
    },
}

impl Header {
    /// Returns the header name.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Plain { name, .. } | Self::Secret { name, .. } => name,
        }
    }

    /// Returns `true` when this header holds a secret.
    #[must_use]
    pub const fn is_secret(&self) -> bool {
        matches!(self, Self::Secret { .. })
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain { name, value } => f
                .debug_struct("Header")
                .field("name", name)
                .field("value", value)
                .finish(),
            Self::Secret { name, .. } => f
                .debug_struct("Header")
                .field("name", name)
                .field("value", &"<redacted>")
                .finish(),
        }
    }
}

/// One outbound HTTP POST request.
#[derive(Debug)]
pub struct HttpRequest {
    /// Absolute endpoint URL.
    pub url: String,
    /// Request headers.
    pub headers: Vec<Header>,
    /// Exact request body bytes.
    pub body: Vec<u8>,
    /// Request timeout in seconds.
    pub timeout_s: u32,
}

impl HttpRequest {
    /// Constructs a request with no headers.
    #[must_use]
    pub fn new(url: impl Into<String>, body: Vec<u8>, timeout_s: u32) -> Self {
        Self {
            url: url.into(),
            headers: Vec::new(),
            body,
            timeout_s,
        }
    }

    /// Appends a non-secret header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header::Plain {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Appends a secret header.
    pub fn secret_header(mut self, name: impl Into<String>, key: Key) -> Self {
        self.headers.push(Header::Secret {
            name: name.into(),
            key,
        });
        self
    }
}

/// One HTTP reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    /// HTTP status code.
    pub status: u16,
    /// Reply body bytes.
    pub body: Vec<u8>,
    /// Input token count reported by the endpoint (`0` when unreported).
    pub tokens_in: u64,
    /// Output token count reported by the endpoint (`0` when unreported).
    pub tokens_out: u64,
    /// `Retry-After` seconds when the endpoint sent one.
    pub retry_after_s: Option<u32>,
    /// Whether the endpoint reported a length-limited reply.
    pub truncated: bool,
}

impl HttpReply {
    /// Constructs a reply with a status and body and no token counts.
    #[must_use]
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            tokens_in: 0,
            tokens_out: 0,
            retry_after_s: None,
            truncated: false,
        }
    }
}

/// Failure of one HTTP POST. It never carries a header value or a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    /// The connection could not be opened.
    Connect,
    /// The request timed out.
    Timeout,
    /// The TLS handshake or certificate check failed.
    Tls,
    /// The reply exceeded the reply-size cap.
    TooLarge,
    /// Any other transport failure.
    Other,
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect => f.write_str("connection failed"),
            Self::Timeout => f.write_str("request timed out"),
            Self::Tls => f.write_str("TLS failure"),
            Self::TooLarge => f.write_str("reply exceeded the size cap"),
            Self::Other => f.write_str("transport failure"),
        }
    }
}

impl std::error::Error for HttpError {}

/// The one outbound transport door. Every send takes the gate's [`Allowed`] token and
/// [`HttpPost::post`] checks it against the URL and the exact body before any transport code
/// runs.
///
/// An implementation provides [`HttpPost::send`] and must not override [`HttpPost::post`]; a
/// real transport (`net-stack-spike`) also refuses redirects, a proxy, and a URL with a user,
/// query or fragment.
pub trait HttpPost {
    /// Sends `req` and returns the reply. Called only by [`HttpPost::post`], after the token
    /// check.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError`] when the request cannot be completed.
    fn send(&self, req: &HttpRequest) -> Result<HttpReply, HttpError>;

    /// Checks that `allowed` covers this endpoint and these exact body bytes, then sends.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError::Other`] without sending when the token does not cover the request or
    /// the URL carries a user, a query or a fragment; otherwise the error of
    /// [`HttpPost::send`].
    fn post(&self, allowed: &Allowed, req: &HttpRequest) -> Result<HttpReply, HttpError> {
        let target = Target::Endpoint(req.url.clone());
        if !url_is_plain(&req.url)
            || !allowed.permits_transfer(&target, &Operation::Post, &req.body)
        {
            return Err(HttpError::Other);
        }
        self.send(req)
    }
}

/// Whether `url` has no user info, query or fragment.
fn url_is_plain(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    !authority.contains('@') && !url.contains('?') && !url.contains('#')
}

/// A request as recorded by [`ScriptedHttp`]: header names only, never a secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    /// Endpoint URL.
    pub url: String,
    /// Header names in order.
    pub header_names: Vec<String>,
    /// Exact body bytes.
    pub body: Vec<u8>,
}

/// A queue of canned replies and a record of requests, with a send counter.
#[derive(Debug, Default)]
pub struct ScriptedHttp {
    replies: Mutex<VecDeque<Result<HttpReply, HttpError>>>,
    requests: Mutex<Vec<RecordedRequest>>,
    sends: AtomicUsize,
}

impl ScriptedHttp {
    /// Constructs an empty scripted transport.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues a successful reply.
    pub fn push(&self, reply: HttpReply) {
        if let Ok(mut queue) = self.replies.lock() {
            queue.push_back(Ok(reply));
        }
    }

    /// Queues a transport failure.
    pub fn push_error(&self, error: HttpError) {
        if let Ok(mut queue) = self.replies.lock() {
            queue.push_back(Err(error));
        }
    }

    /// Returns the recorded requests in send order.
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Returns how many sends were attempted.
    #[must_use]
    pub fn sends(&self) -> usize {
        self.sends.load(Ordering::SeqCst)
    }
}

impl HttpPost for ScriptedHttp {
    fn send(&self, req: &HttpRequest) -> Result<HttpReply, HttpError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(RecordedRequest {
                url: req.url.clone(),
                header_names: req.headers.iter().map(|h| h.name().to_owned()).collect(),
                body: req.body.clone(),
            });
        }
        let next = self
            .replies
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        match next {
            Some(result) => result,
            None => Err(HttpError::Connect),
        }
    }
}

/// A transport whose every send fails the test.
#[derive(Debug, Clone, Copy, Default)]
pub struct PanicHttp;

impl HttpPost for PanicHttp {
    fn send(&self, _req: &HttpRequest) -> Result<HttpReply, HttpError> {
        panic!("a send was attempted")
    }
}

/// What this build can do, passed explicitly so a test can choose the values it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Avail {
    /// Whether the `net` feature is compiled in.
    pub net_built: bool,
    /// Whether the `bridge` feature is compiled in.
    pub bridge_built: bool,
    /// Whether the CPU supports the features an adapter needs.
    pub cpu_ok: bool,
}

impl Avail {
    /// Reads the compiled features and this machine's CPU support.
    #[must_use]
    pub fn real() -> Self {
        Self {
            net_built: cfg!(feature = "net"),
            bridge_built: cfg!(feature = "bridge"),
            cpu_ok: cpu_ok(),
        }
    }
}

/// Whether this machine's CPU supports the features the adapters require.
///
/// The first adapters need no CPU feature; `net-stack-spike` gives this function its body.
#[must_use]
pub const fn cpu_ok() -> bool {
    true
}
