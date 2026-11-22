use desafinado_core::log_err::LogErr;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub chunks: Vec<(Duration, Vec<u8>)>,
    pub content_length: bool,
}

impl Reply {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            chunks: vec![(Duration::ZERO, body.as_bytes().to_vec())],
            content_length: true,
        }
    }

    pub fn sse(parts: &[&str]) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            chunks: parts
                .iter()
                .map(|part| (Duration::ZERO, part.as_bytes().to_vec()))
                .collect(),
            content_length: false,
        }
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        if let Some(first) = self.chunks.first_mut() {
            first.0 = delay;
        }
        self
    }
}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub struct TestServer {
    pub addr: SocketAddr,
    pub requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut start = String::new();
    reader.read_line(&mut start).ok()?;
    let mut parts = start.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        headers.insert(name.trim().to_lowercase(), value.trim().to_owned());
    }
    let length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(RecordedRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn write_reply(stream: &mut TcpStream, reply: &Reply) {
    let total: usize = reply.chunks.iter().map(|(_, c)| c.len()).sum();
    let mut head = format!("HTTP/1.1 {} X\r\nConnection: close\r\n", reply.status);
    for (name, value) in &reply.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    if reply.content_length {
        head.push_str("Content-Length: ");
        head.push_str(&total.to_string());
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    for (delay, chunk) in &reply.chunks {
        std::thread::sleep(*delay);
        if stream.write_all(chunk).is_err() || stream.flush().is_err() {
            return;
        }
    }
}

impl TestServer {
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let requests: Arc<Mutex<Vec<RecordedRequest>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let queue = Arc::new(Mutex::new(replies.into_iter()));
        let (log, flag) = (Arc::clone(&requests), Arc::clone(&stop));
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = connection else { continue };
                let (queue, log) = (Arc::clone(&queue), Arc::clone(&log));
                std::thread::spawn(move || {
                    let Some(request) = read_request(&mut stream) else {
                        return;
                    };
                    log.lock().unwrap().push(request);
                    let reply = queue.lock().unwrap().next().unwrap_or_else(|| {
                        Reply::json(404, r#"{"error":{"message":"no more replies","code":404}}"#)
                    });
                    write_reply(&mut stream, &reply);
                });
            }
        });
        Self {
            addr,
            requests,
            stop,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/api/v1", self.addr)
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        TcpStream::connect(self.addr).log_warn();
    }
}
