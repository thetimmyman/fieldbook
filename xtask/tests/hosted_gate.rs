//! Hosted login-gate check integration tests (`hosted-gate`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "Hosted login-gate check integration tests start a mock server and run bash/curl"
)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, sleep, spawn};
use std::time::Duration;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn tools_available() -> bool {
    let bash_ok = Command::new("bash")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let curl_ok = Command::new("curl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    bash_ok && curl_ok
}

fn require_tools() {
    if !tools_available() {
        panic!("bash and curl are required for the login-gate tests");
    }
}

struct MockServer {
    port: u16,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockServer {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(e) => panic!("bind tcp listener: {e}"),
        };
        let port = match listener.local_addr() {
            Ok(a) => a.port(),
            Err(e) => panic!("local addr: {e}"),
        };
        if let Err(e) = listener.set_nonblocking(true) {
            panic!("set listener nonblocking: {e}");
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let handler = Arc::new(handler);

        let handle = spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let mut buf = [0u8; 1024];
                        if let Ok(n) = stream.read(&mut buf)
                            && n > 0
                        {
                            let req = String::from_utf8_lossy(&buf[..n]);
                            let first_line = req.lines().next().unwrap_or("");
                            let parts: Vec<&str> = first_line.split_whitespace().collect();
                            let path = if parts.len() >= 2 { parts[1] } else { "/" };
                            let response = handler(path);
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            running,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = TcpStream::connect(format!("127.0.0.1:{}", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_gate_script(args: &[&str]) -> (ExitStatus, String, String) {
    let script = repo_root().join("deploy/check-login-gate.sh");
    let out = match Command::new("bash").arg(&script).args(args).output() {
        Ok(o) => o,
        Err(e) => panic!("failed to run check-login-gate.sh: {e}"),
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status, stdout, stderr)
}

#[test]
fn gate_script_all_paths_pass_on_302_to_access() {
    require_tools();
    let server = MockServer::start(|_path| {
        "HTTP/1.1 302 Found\r\nLocation: https://example-team.cloudflareaccess.com/cdn-cgi/access/login/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert!(
        status.success(),
        "expected script exit 0, got {:?}, stdout: {stdout}, stderr: {stderr}",
        status.code()
    );
    assert!(stdout.contains("ok /: 302 to the login"));
    assert!(stdout.contains("ok /index.html: 302 to the login"));
    assert!(stdout.contains("ok /assets/app.css: 302 to the login"));
}

#[test]
fn gate_script_reads_a_lowercase_location_header() {
    require_tools();
    let server = MockServer::start(|_path| {
        "HTTP/1.1 302 Found\r\nlocation: https://example-team.cloudflareaccess.com/cdn-cgi/access/login/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert!(
        status.success(),
        "expected script exit 0, got {:?}, stdout: {stdout}, stderr: {stderr}",
        status.code()
    );
    assert!(stdout.contains("ok /: 302 to the login"));
    assert!(stdout.contains("ok /index.html: 302 to the login"));
    assert!(stdout.contains("ok /assets/app.css: 302 to the login"));
}

// control: HO-03
#[test]
fn gate_script_negative_control_fails_on_200_with_edition_content() {
    require_tools();
    let server = MockServer::start(|_path| {
        let body = "<title>Fieldbook</title>";
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert_eq!(
        status.code(),
        Some(1),
        "expected exit 1 on 200 with edition body, got: {status:?}, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("FAIL /: status 200"));
}

// control: HO-03
#[test]
fn gate_script_fails_when_path_answers_302_with_location_on_another_host() {
    require_tools();
    let server = MockServer::start(|path| {
        if path == "/index.html" {
            "HTTP/1.1 302 Found\r\nLocation: https://other-domain.example.com/login\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        } else {
            "HTTP/1.1 302 Found\r\nLocation: https://example-team.cloudflareaccess.com/cdn-cgi/access/login/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        }
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert_eq!(
        status.code(),
        Some(1),
        "expected exit 1 on other host location, got: {status:?}, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("FAIL /index.html: Location is not on the login host"));
}

// control: HO-03
#[test]
fn gate_script_fails_when_a_redirect_carries_no_location() {
    require_tools();
    let server = MockServer::start(|_path| {
        "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    });
    let (status, stdout, stderr) = run_gate_script(&[&server.url(), "/", "/index.html"]);
    assert_eq!(
        status.code(),
        Some(1),
        "expected exit 1 on a redirect without a Location, got: {status:?}, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("FAIL /: redirect without a Location"));
}

// control: HO-03
#[test]
fn gate_script_fails_on_403_whose_body_contains_fieldbook() {
    require_tools();
    let server = MockServer::start(|_path| {
        let body = "Fieldbook";
        format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert_eq!(
        status.code(),
        Some(1),
        "expected exit 1 on 403 with Fieldbook in body, got: {status:?}, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("FAIL /: body carries edition content"));
}

#[test]
fn gate_script_passes_on_403_whose_body_is_forbidden() {
    require_tools();
    let server = MockServer::start(|_path| {
        let body = "Forbidden";
        format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert!(
        status.success(),
        "expected exit 0 on 403 with Forbidden body, got: {status:?}, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("ok /: 403 status"));
    assert!(stdout.contains("ok /index.html: 403 status"));
    assert!(stdout.contains("ok /assets/app.css: 403 status"));
}

#[test]
fn gate_script_output_never_contains_host_port_or_location() {
    require_tools();
    let location_val = "https://example-team.cloudflareaccess.com/cdn-cgi/access/login/x";
    let server = MockServer::start(move |_path| {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {location_val}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    });
    let (status, stdout, stderr) =
        run_gate_script(&[&server.url(), "/", "/index.html", "/assets/app.css"]);
    assert!(status.success());

    let combined = format!("{stdout}\n{stderr}");
    let host_port = format!("127.0.0.1:{}", server.port);
    assert!(
        !combined.contains(&host_port),
        "output must not contain server 127.0.0.1:PORT (`{host_port}`)"
    );
    assert!(
        !combined.contains(location_val),
        "output must not contain Location value"
    );
    assert!(
        !combined.contains("example-team.cloudflareaccess.com"),
        "output must not contain Location hostname"
    );
}

#[test]
fn gate_script_called_with_one_argument_exits_2() {
    require_tools();
    let (status, _stdout, stderr) = run_gate_script(&["http://127.0.0.1:1234"]);
    assert_eq!(
        status.code(),
        Some(2),
        "expected exit 2 on fewer than 2 args, got {status:?}, stderr: {stderr}"
    );
}

#[test]
fn gate_script_does_not_need_gnu_awk() {
    let script_path = repo_root().join("deploy/check-login-gate.sh");
    let content = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", script_path.display()));
    assert!(
        !content.contains("awk"),
        "deploy/check-login-gate.sh must not contain `awk`"
    );
    assert!(
        !content.contains("IGNORECASE"),
        "deploy/check-login-gate.sh must not contain `IGNORECASE`"
    );
}
