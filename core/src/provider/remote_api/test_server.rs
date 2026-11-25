//! A local HTTP/1.1 server for client tests, with optional TLS and one request per connection.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use rustls::{ServerConfig, ServerConnection, StreamOwned, pki_types::PrivatePkcs8KeyDer};

#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub method: String,
    pub body: Vec<u8>,
    /// The request target as sent: path and query, still encoded.
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// A decoded query value.
    pub(crate) fn query(&self, name: &str) -> Option<String> {
        let url = reqwest::Url::parse(&format!("http://local{}", self.target)).ok()?;
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }
}

pub(crate) struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    /// A `Content-Length` larger than the body, so the body ends early.
    declared_length: Option<usize>,
    /// How long to keep the connection open after the body before closing it.
    linger: Duration,
    /// How long to wait before sending anything.
    delay: Duration,
}

impl Reply {
    pub(crate) fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into(),
            declared_length: None,
            linger: Duration::ZERO,
            delay: Duration::ZERO,
        }
    }

    pub(crate) fn html(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "text/html".into())],
            body: body.into(),
            declared_length: None,
            linger: Duration::ZERO,
            delay: Duration::ZERO,
        }
    }

    pub(crate) fn redirect(location: impl Into<String>) -> Self {
        Self::redirect_with(302, location)
    }

    pub(crate) fn redirect_with(status: u16, location: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![("Location".into(), location.into())],
            body: String::new(),
            declared_length: None,
            linger: Duration::ZERO,
            delay: Duration::ZERO,
        }
    }

    /// Announces `length` bytes but sends only the body, then closes after `linger`: a body
    /// that is cut off, or with a long `linger` one that stalls.
    pub(crate) fn cut_off(mut self, length: usize, linger: Duration) -> Self {
        self.declared_length = Some(length);
        self.linger = linger;
        self
    }

    /// Sends the answer only after `delay`.
    pub(crate) fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub(crate) fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

pub(crate) struct TestServer {
    /// The loopback origin, with HTTPS for TLS-enabled servers.
    pub url: String,
    pub port: u16,
    requests: Arc<Mutex<Vec<Request>>>,
    stopped: Arc<AtomicBool>,
}

impl TestServer {
    pub(crate) fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        Self::listen(None, handler)
    }

    /// Accepts TLS and plaintext on the same port, so an origin test can change only the scheme.
    pub(crate) fn start_tls(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        Self::listen(Some(tls().config.clone()), handler)
    }

    fn listen(
        tls: Option<Arc<ServerConfig>>,
        handler: impl Fn(&Request) -> Reply + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let port = listener.local_addr().expect("local address").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let base = if tls.is_some() {
            "https://127.0.0.1"
        } else {
            "http://127.0.0.1"
        };
        let mut url = reqwest::Url::parse(base).unwrap();
        url.set_port(Some(port)).unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                // TLS begins with a handshake record. Plain HTTP starts with the method.
                let mut prefix = [0];
                if let Some(config) = &tls
                    && stream.peek(&mut prefix).ok() == Some(1)
                    && prefix == [0x16]
                {
                    let connection = ServerConnection::new(config.clone()).unwrap();
                    serve(StreamOwned::new(connection, stream), &recorded, &handler);
                } else {
                    serve(stream, &recorded, &handler);
                }
            }
        });
        Self {
            url: url.origin().ascii_serialization(),
            port,
            requests,
            stopped,
        }
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

/// A client that trusts only the loopback certificate generated for these tests.
pub(crate) fn tls_client_builder() -> reqwest::ClientBuilder {
    crate::provider::http_client_builder().tls_certs_only([tls().root.clone()])
}

struct TestTls {
    config: Arc<ServerConfig>,
    root: reqwest::Certificate,
}

fn tls() -> &'static TestTls {
    static TLS: OnceLock<TestTls> = OnceLock::new();
    TLS.get_or_init(|| {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
        let root = reqwest::Certificate::from_der(cert.der()).unwrap();
        let key = PrivatePkcs8KeyDer::from(signing_key.serialize_der());
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], key.into())
            .unwrap();
        TestTls {
            config: Arc::new(config),
            root,
        }
    })
}

fn serve(
    mut stream: impl Read + Write,
    recorded: &Mutex<Vec<Request>>,
    handler: &impl Fn(&Request) -> Reply,
) {
    let Some(request) = read_request(&mut BufReader::new(&mut stream)) else {
        return;
    };
    recorded.lock().unwrap().push(request.clone());
    let reply = handler(&request);
    thread::sleep(reply.delay);
    let mut head = format!(
        "HTTP/1.1 {} Test\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.declared_length.unwrap_or(reply.body.len())
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(reply.body.as_bytes());
    let _ = stream.flush();
    thread::sleep(reply.linger);
}

impl Drop for TestServer {
    /// Ends the listener thread: it sees the flag on the next connection, made here.
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn read_request(reader: &mut impl BufRead) -> Option<Request> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut start = line.split_whitespace();
    let method = start.next()?.to_string();
    let target = start.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut request = Request {
        method,
        body: Vec::new(),
        target,
        headers,
    };
    if let Some(length) = request.header("content-length") {
        let length = length.parse().ok()?;
        request.body.resize(length, 0);
        reader.read_exact(&mut request.body).ok()?;
    }
    Some(request)
}

/// A port nothing listens on.
pub(crate) fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("bind a local port")
        .port()
}
