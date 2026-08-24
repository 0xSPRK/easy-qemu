use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

const IO_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Асинхронный QMP-клиент (одно соединение = одна сессия).
pub struct Qmp {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Qmp {
    pub async fn connect(addr: SocketAddr) -> Result<Qmp> {
        let raw = timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await??;
        raw.set_nodelay(true).ok();
        let (r, w) = raw.into_split();
        let mut reader = BufReader::new(r);
        // Greeting от сервера.
        read_msg(&mut reader).await?;
        let mut q = Qmp { reader, writer: w };
        // qmp_capabilities обязана быть первой командой.
        q.exec("qmp_capabilities", None).await?;
        Ok(q)
    }

    pub async fn exec(&mut self, cmd: &str, args: Option<Value>) -> Result<Value> {
        let req = match args {
            Some(a) => serde_json::json!({ "execute": cmd, "arguments": a }),
            None => serde_json::json!({ "execute": cmd }),
        };
        let line = serde_json::to_string(&req)?;
        write_line(&mut self.writer, &line).await?;

        loop {
            let v = read_msg(&mut self.reader).await?;
            if v.get("event").is_some() {
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(anyhow!("QMP error: {err}"));
            }
            if v.get("return").is_some() {
                return Ok(v["return"].clone());
            }
        }
    }

    /// Реальный VNC-порт из `query-vnc` (поле service).
    pub async fn query_vnc_port(&mut self) -> Result<Option<u16>> {
        let ret = match self.exec("query-vnc", None).await {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        if ret.get("enabled").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let port = ret
            .get("service")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u16>().ok());
        Ok(port.or_else(|| ret.get("service").and_then(Value::as_u64).map(|p| p as u16)))
    }

    /// Статус гостя: running / paused / shutdown и т.п.
    pub async fn query_status(&mut self) -> Result<String> {
        let ret = self.exec("query-status", None).await?;
        Ok(ret
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }
}

async fn write_line(w: &mut OwnedWriteHalf, line: &str) -> Result<()> {
    timeout(IO_TIMEOUT, async {
        w.write_all(line.as_bytes()).await?;
        w.write_all(b"\n").await?;
        w.flush().await
    })
    .await??;
    Ok(())
}

async fn read_msg<R>(r: &mut R) -> Result<Value>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        let n = timeout(IO_TIMEOUT, r.read_line(&mut line)).await??;
        if n == 0 {
            return Err(anyhow!("QMP connection closed"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(serde_json::from_str(trimmed)?);
    }
}
