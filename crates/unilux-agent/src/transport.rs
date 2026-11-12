//! Newline-delimited JSON-RPC over stdio and a Unix socket.

use std::path::Path;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::Server;

async fn serve_stream<R, W>(server: Server, r: R, mut w: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let mut out = Vec::new();
                for m in batch {
                    if let Some(r) = server.handle(m).await {
                        out.push(r);
                    }
                }
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => server.handle(msg).await,
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}),
            ),
        };
        if let Some(r) = reply {
            w.write_all(format!("{r}\n").as_bytes()).await?;
            w.flush().await?;
        }
    }
    Ok(())
}

/// Serve on stdin/stdout until stdin closes.
pub async fn serve_stdio(server: Server) -> std::io::Result<()> {
    serve_stream(server, tokio::io::stdin(), tokio::io::stdout()).await
}

/// Accept connections on a Unix socket (mode 0600) until the task is
/// dropped. A stale socket file from a crashed run is replaced.
pub async fn serve_socket(server: Server, path: &Path) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700));
    }
    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another Unilux is already serving this socket",
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    loop {
        let (stream, _) = listener.accept().await?;
        let s = server.clone();
        tokio::spawn(async move {
            let (r, w) = stream.into_split();
            let _ = serve_stream(s, r, w).await;
        });
    }
}

/// Forward stdio to a running Unilux's socket. Returns false when nothing is
/// listening, so the caller can serve standalone instead.
pub async fn proxy_stdio_to_socket(path: &Path) -> std::io::Result<bool> {
    let Ok(stream) = UnixStream::connect(path).await else {
        return Ok(false);
    };
    let (mut r, mut w) = stream.into_split();
    let up = async {
        tokio::io::copy(&mut tokio::io::stdin(), &mut w).await?;
        w.shutdown().await
    };
    let mut stdout = tokio::io::stdout();
    let down = tokio::io::copy(&mut r, &mut stdout);
    let (a, b) = tokio::join!(up, down);
    a?;
    b?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn socket_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().display().to_string();
        let paths = unilux_core::Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let svc = unilux_service::Service::builder(paths)
            .host(Arc::new(unilux_system::FakeHost::new()))
            .log_file(false)
            .build()
            .unwrap();
        let sock = d.path().join("run/unilux.sock");
        let sock2 = sock.clone();
        tokio::spawn(async move { serve_socket(Server::new(svc).without_audit(), &sock2).await });
        for _ in 0..50 {
            if sock.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let s = UnixStream::connect(&sock).await.unwrap();
        let (r, mut w) = s.into_split();
        w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\nnot json\n")
            .await
            .unwrap();
        let mut lines = BufReader::new(r).lines();
        let a: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(a["id"], 7);
        let b: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(b["error"]["code"], -32700);
    }
}
