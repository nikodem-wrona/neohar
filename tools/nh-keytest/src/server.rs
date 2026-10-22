//! A small HTTP server for the browser test. It has no async runtime: one thread for each
//! connection, and every response closes the connection.
//!
//! - `GET /ready`: `ready`, for the test runner.
//! - `GET /cases`: the cases of the matrix as JSON, read from disk at each request.
//! - `POST /encode`: a [`crate::Request`] as JSON in, an [`crate::Encoded`] as JSON out.
//! - `GET <path>`: a file of the web directory. `/` is `test.html`.

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

use crate::{Request, load_cases, run};

/// The most bytes of a request that the server reads.
const MAX_REQUEST: usize = 4 << 20;

/// Where the server finds its files.
pub struct Config {
    /// The built test page: `test.html`, `test.js`, `capture.html`, `capture.js`.
    pub web: PathBuf,
    /// The directory of the `*.toml` cases.
    pub cases: PathBuf,
}

/// Serves connections until the process ends.
pub fn serve(listener: TcpListener, config: Config) {
    let config = Arc::new(config);
    for stream in listener.incoming().filter_map(Result::ok) {
        let config = Arc::clone(&config);
        thread::spawn(move || {
            // A client that goes away in the middle of a response is not an error of the server.
            let _ = handle(stream, &config);
        });
    }
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn text(status: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }
}

fn handle(mut stream: TcpStream, config: &Config) -> io::Result<()> {
    let response = match read_request(&mut stream) {
        Ok((method, path, body)) => route(config, &method, &path, &body),
        Err(message) => Response::text("400 Bad Request", &message),
    };
    let head = format!(
        "HTTP/1.1 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)
}

/// Reads the method, the path without the query, and the body.
fn read_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>), String> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if data.len() > MAX_REQUEST {
            return Err("request too large".into());
        }
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the connection closed in the header".into());
        }
        data.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut parts = lines.next().unwrap_or_default().split_whitespace();
    let method = parts.next().ok_or("no method")?.to_owned();
    let target = parts.next().ok_or("no path")?;
    let path = target.split('?').next().unwrap_or(target).to_owned();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map_or(Ok(0), |(_, value)| value.trim().parse::<usize>())
        .map_err(|_| "bad content-length")?;
    if length > MAX_REQUEST {
        return Err("request too large".into());
    }
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the connection closed in the body".into());
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok((method, path, body))
}

fn route(config: &Config, method: &str, path: &str, body: &[u8]) -> Response {
    match (method, path) {
        ("GET", "/ready") => Response::text("200 OK", "ready"),
        ("GET", "/cases") => match load_cases(&config.cases) {
            Ok(cases) => json(&cases),
            Err(e) => Response::text("500 Internal Server Error", &e.to_string()),
        },
        ("POST", "/encode") => match serde_json::from_slice::<Request>(body) {
            Ok(request) => match run(&request) {
                Ok(encoded) => json(&encoded),
                Err(e) => Response::text("400 Bad Request", &e.to_string()),
            },
            Err(e) => Response::text("400 Bad Request", &e.to_string()),
        },
        ("GET", path) => file(&config.web, path),
        _ => Response::text("405 Method Not Allowed", "method not allowed"),
    }
}

fn json<T: serde::Serialize>(value: &T) -> Response {
    match serde_json::to_vec(value) {
        Ok(body) => Response {
            status: "200 OK",
            content_type: "application/json",
            body,
        },
        Err(e) => Response::text("500 Internal Server Error", &e.to_string()),
    }
}

fn file(web: &Path, path: &str) -> Response {
    let name = if path == "/" { "/test.html" } else { path };
    if name.contains("..") || name.contains('\\') || !name.starts_with('/') {
        return Response::text("404 Not Found", "not found");
    }
    let content_type = match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    };
    match std::fs::read(web.join(&name[1..])) {
        Ok(body) => Response {
            status: "200 OK",
            content_type,
            body,
        },
        Err(_) => Response::text("404 Not Found", "not found"),
    }
}
