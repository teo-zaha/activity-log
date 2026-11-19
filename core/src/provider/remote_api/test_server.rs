//! A local HTTP/1.1 server for client tests: each request is answered by a handler and
//! recorded. No TLS, one request per connection.

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Debug, Clone)]
pub(crate) struct Request {
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
    /// `http://127.0.0.1:{port}`
    pub url: String,
    pub port: u16,
    requests: Arc<Mutex<Vec<Request>>>,
    stopped: Arc<AtomicBool>,
}

impl TestServer {
    pub(crate) fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
        let port = listener.local_addr().expect("local address").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(request) = read_request(&mut BufReader::new(&stream)) else {
                    continue;
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
        });
        Self {
            url: format!("http://127.0.0.1:{port}"),
            port,
            requests,
            stopped,
        }
    }

    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
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
    let target = line.split_whitespace().nth(1)?.to_string();
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
    Some(Request { target, headers })
}

/// A port nothing listens on.
pub(crate) fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("bind a local port")
        .port()
}
