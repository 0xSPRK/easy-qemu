use std::collections::HashMap;

use anyhow::Result;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A single WS→TCP proxy (websockify) for one VM.
struct Srv {
    port: u16,
    stop_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct Target {
    vnc_host: String,
    vnc_port: u16,
}

/// Pool of websockify proxies keyed by VM id.
///
/// Tasks run on the ambient tokio runtime when one exists (Tauri/tests);
/// otherwise a lazily created owned runtime is used. Static noVNC files are
/// NOT served — the RFB library is bundled by the frontend, only the
/// WebSocket endpoint lives here.
pub struct ProxyPool {
    owned_rt: Option<tokio::runtime::Runtime>,
    servers: HashMap<String, Srv>,
}

impl ProxyPool {
    pub fn new() -> ProxyPool {
        ProxyPool {
            owned_rt: None,
            servers: HashMap::new(),
        }
    }

    pub fn port_of(&self, id: &str) -> Option<u16> {
        self.servers.get(id).map(|s| s.port)
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.servers.contains_key(id)
    }

    /// Starts the proxy if it is not running yet; returns the port.
    pub async fn ensure(&mut self, id: &str, vnc_host: String, vnc_port: u16) -> Result<u16> {
        if let Some(s) = self.servers.get(id) {
            return Ok(s.port);
        }
        let target = Target { vnc_host, vnc_port };

        let bind = || async {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
            let port = listener.local_addr()?.port();
            Ok::<_, std::io::Error>((listener, port))
        };

        let (listener, port) = match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                let joined = h.spawn(bind()).await.map_err(|e| anyhow::anyhow!("{e}"))?;
                joined?
            }
            Err(_) => self.ensure_owned().block_on(bind())?,
        };

        let (stop_tx, stop_rx) = watch::channel(false);
        let app = Router::new()
            .route("/websockify", get(ws_handler))
            .with_state(target);

        let serve = async move {
            let mut rx = stop_rx;
            let shutdown = async move {
                loop {
                    if rx.changed().await.is_err() || *rx.borrow() {
                        break;
                    }
                }
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await;
        };

        let handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h.spawn(serve),
            Err(_) => self.ensure_owned().spawn(serve),
        };

        self.servers.insert(
            id.to_string(),
            Srv {
                port,
                stop_tx,
                handle,
            },
        );
        Ok(port)
    }

    fn ensure_owned(&mut self) -> &tokio::runtime::Runtime {
        if self.owned_rt.is_none() {
            self.owned_rt = Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("eq-proxy")
                    .enable_all()
                    .build()
                    .expect("failed to create tokio runtime for websockify"),
            );
        }
        self.owned_rt.as_ref().unwrap()
    }

    pub fn stop(&mut self, id: &str) -> Result<()> {
        if let Some(srv) = self.servers.remove(id) {
            let _ = srv.stop_tx.send(true);
            srv.handle.abort();
        }
        Ok(())
    }

    pub fn shutdown_all(&mut self) {
        let ids: Vec<String> = self.servers.keys().cloned().collect();
        for id in ids {
            let _ = self.stop(&id);
        }
    }
}

impl Default for ProxyPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProxyPool {
    fn drop(&mut self) {
        self.shutdown_all();
        // Do not drop our own runtime from someone else's async context — let the OS reclaim it.
        if let Some(rt) = self.owned_rt.take() {
            std::mem::forget(rt);
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(t): State<Target>) -> Response {
    ws.on_upgrade(move |socket| proxy(socket, t.vnc_host, t.vnc_port))
}

async fn proxy(ws: WebSocket, vnc_host: String, vnc_port: u16) {
    let tcp = match TcpStream::connect((vnc_host.as_str(), vnc_port)).await {
        Ok(t) => t,
        Err(_) => return,
    };
    let (ws_sender, ws_receiver) = ws.split();
    let (tcp_read, tcp_write) = tcp.into_split();

    tokio::select! {
        _ = pump_ws_to_tcp(ws_receiver, tcp_write) => {}
        _ = pump_tcp_to_ws(tcp_read, ws_sender) => {}
    }
}

/// WebSocket → TCP: client input (keyboard/mouse) goes to the VNC server.
async fn pump_ws_to_tcp(mut rx: SplitStream<WebSocket>, mut w: OwnedWriteHalf) {
    while let Some(Ok(msg)) = rx.next().await {
        let payload: Vec<u8> = match msg {
            Message::Text(t) => t.as_bytes().to_vec(),
            Message::Binary(b) => b.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };
        if w.write_all(&payload).await.is_err() {
            break;
        }
    }
    let _ = w.shutdown().await;
}

/// TCP → WebSocket: framebuffer updates go back to the browser.
async fn pump_tcp_to_ws(mut r: OwnedReadHalf, mut tx: SplitSink<WebSocket, Message>) {
    let mut buf = [0u8; 16384];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let payload = Bytes::copy_from_slice(&buf[..n]);
                if tx.send(Message::Binary(payload)).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = tx.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::time::Duration;

    #[tokio::test(flavor = "multi_thread")]
    async fn websockify_proxies_bytes_to_vnc() {
        let vnc_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let vnc_port = vnc_listener.local_addr().unwrap().port();

        let mut pool = ProxyPool::new();
        let port = pool
            .ensure("vm-proxy", "127.0.0.1".into(), vnc_port)
            .await
            .unwrap();

        let echo_thread = std::thread::spawn(move || {
            let (mut s, _) = vnc_listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            loop {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                s.write_all(&buf[..n]).unwrap();
            }
        });

        let mut sock = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        sock.set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let req = format!(
            "GET /websockify HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        sock.write_all(req.as_bytes()).unwrap();

        let mut headers = String::new();
        let mut byte = [0u8; 1];
        while !headers.contains("\r\n\r\n") {
            sock.read_exact(&mut byte).unwrap();
            headers.push(byte[0] as char);
        }
        assert!(
            headers.starts_with("HTTP/1.1 101"),
            "upgrade failed: {headers}"
        );

        let payload = [1u8, 2, 3, 4, 5];
        let mask = [0x11u8, 0x22, 0x33, 0x44];
        let mut frame = vec![0x82, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        sock.write_all(&frame).unwrap();

        let mut hdr = [0u8; 2];
        sock.read_exact(&mut hdr).unwrap();
        assert_eq!(hdr[0] & 0x0f, 2, "expected binary frame");
        let len = hdr[1] as usize;
        assert!(len < 126);
        let mut resp = vec![0u8; len];
        sock.read_exact(&mut resp).unwrap();
        assert_eq!(&resp[..], &payload[..]);

        let _ = sock.write_all(&[0x88, 0x00]);
        drop(sock);
        echo_thread.join().unwrap();
        pool.stop("vm-proxy").unwrap();
    }

    #[tokio::test]
    async fn ensure_is_idempotent() {
        let mut pool = ProxyPool::new();
        let p1 = pool.ensure("a", "127.0.0.1".into(), 5999).await.unwrap();
        let p2 = pool.ensure("a", "127.0.0.1".into(), 5999).await.unwrap();
        assert_eq!(p1, p2);
        pool.shutdown_all();
        assert!(!pool.is_running("a"));
    }
}
