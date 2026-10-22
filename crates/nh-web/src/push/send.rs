//! The POST of a push to the push service (RFC 8030).
//!
//! One HTTP/1.1 request on a new connection, through the client of `hyper` over `rustls` with the
//! provider `ring` and the roots of `webpki-roots`. A push is rare, so the sender keeps no
//! connection. The test origin of a development build is plain HTTP.

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{Request, header};
use hyper_util::rt::TokioIo;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use super::allow::Target;

/// How long one try may take: the connection, TLS and the answer.
const TRY_TIMEOUT: Duration = Duration::from_secs(30);

/// The most of the body of an answer that the sender reads, for the log.
const ANSWER_MAX: usize = 4096;

/// What a push service answered.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    /// The start of the body, on one line: the reason of a refusal.
    pub text: String,
}

/// The parts of one POST.
pub struct Post<'a> {
    pub target: &'a Target,
    pub authorization: String,
    pub body: Vec<u8>,
    /// `TTL` in seconds.
    pub ttl: u32,
    /// `normal` or `high`.
    pub urgency: &'static str,
    pub topic: Option<&'a str>,
}

/// The TLS client of the push: TLS 1.2 and 1.3, the provider `ring`, the roots of Mozilla, and
/// HTTP/1.1 in ALPN.
pub fn tls_config() -> Result<Arc<ClientConfig>, String> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|error| error.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Sends one POST and reads the answer. `tls` is needed for an `https` target.
pub async fn post(post: Post<'_>, tls: Option<Arc<ClientConfig>>) -> Result<Answer, String> {
    match tokio::time::timeout(TRY_TIMEOUT, send(post, tls)).await {
        Ok(answer) => answer,
        Err(_) => Err(format!("no answer in {} seconds", TRY_TIMEOUT.as_secs())),
    }
}

async fn send(post: Post<'_>, tls: Option<Arc<ClientConfig>>) -> Result<Answer, String> {
    let target = post.target;
    let tcp = TcpStream::connect((target.host.as_str(), target.port))
        .await
        .map_err(|error| format!("no connection to {}: {error}", target.host))?;
    let _ = tcp.set_nodelay(true);
    if !target.tls {
        return exchange(tcp, post).await;
    }
    let config = tls.ok_or("no TLS client")?;
    let name = ServerName::try_from(target.host.clone())
        .map_err(|error| format!("{}: {error}", target.host))?;
    let stream = tokio_rustls::TlsConnector::from(config)
        .connect(name, tcp)
        .await
        .map_err(|error| format!("TLS with {}: {error}", target.host))?;
    exchange(stream, post).await
}

async fn exchange<S>(stream: S, post: Post<'_>) -> Result<Answer, String>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|error| format!("HTTP: {error}"))?;
    let driver = tokio::spawn(connection);
    let mut request = Request::post(post.target.path.as_str())
        .header(header::HOST, post.target.host_header())
        .header(header::AUTHORIZATION, post.authorization)
        .header(header::CONTENT_ENCODING, "aes128gcm")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, post.body.len())
        .header("TTL", post.ttl)
        .header("Urgency", post.urgency);
    if let Some(topic) = post.topic {
        request = request.header("Topic", topic);
    }
    let request = request
        .body(Full::new(Bytes::from(post.body)))
        .map_err(|error| format!("the request: {error}"))?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|error| format!("HTTP: {error}"))?;
    let status = response.status().as_u16();
    let text = match Limited::new(response.into_body(), ANSWER_MAX)
        .collect()
        .await
    {
        Ok(body) => one_line(&body.to_bytes()),
        Err(_) => String::new(),
    };
    driver.abort();
    Ok(Answer { status, text })
}

/// The start of a body as one line of text, for the log.
fn one_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tls_client_has_the_roots_and_http_1_1() {
        let config = tls_config().expect("a TLS client");
        assert_eq!(config.alpn_protocols, [b"http/1.1".to_vec()]);
        assert!(webpki_roots::TLS_SERVER_ROOTS.len() > 100);
    }

    #[test]
    fn the_text_of_an_answer_is_one_short_line() {
        assert_eq!(
            one_line(b"{\n  \"reason\": \"BadJwtToken\"\n}"),
            "{ \"reason\": \"BadJwtToken\" }"
        );
        assert_eq!(one_line(&[b'x'; 1000]).len(), 300);
    }
}
