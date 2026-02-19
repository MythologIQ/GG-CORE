//! Integration tests for the IPC server loop.
//!
//! Tests framing, connection limits, request routing, and shutdown.
//! Platform-specific socket tests use Windows named pipes or Unix sockets
//! depending on the build target.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use veritas_sdr::ipc::{ConnectionConfig, ConnectionPool};

// ---------------------------------------------------------------------------
// Framing helpers (mirrors server.rs read_frame / write_frame)
// ---------------------------------------------------------------------------

async fn write_frame<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    data: &[u8],
) {
    let len = data.len() as u32;
    w.write_all(&len.to_le_bytes()).await.unwrap();
    w.write_all(data).await.unwrap();
    w.flush().await.unwrap();
}

async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await.unwrap();
    buf
}

// ---------------------------------------------------------------------------
// Helpers: build a test IpcHandler via Runtime
// ---------------------------------------------------------------------------

fn test_handler() -> Arc<veritas_sdr::ipc::IpcHandler> {
    let rt = veritas_sdr::Runtime::new(veritas_sdr::RuntimeConfig {
        auth_token: "test-token".into(),
        ..Default::default()
    });
    Arc::new(rt.ipc_handler)
}

// ---------------------------------------------------------------------------
// Unit-level: framing round-trip over tokio::io::duplex
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_framing_roundtrip() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    let payload = b"hello framing";
    write_frame(&mut client, payload).await;

    let received = read_frame(&mut server).await;
    assert_eq!(received, payload);
}

#[tokio::test]
async fn test_framing_empty_payload() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    write_frame(&mut client, b"").await;

    let received = read_frame(&mut server).await;
    assert!(received.is_empty());
}

#[tokio::test]
async fn test_framing_large_payload() {
    let (mut client, mut server) = tokio::io::duplex(128 * 1024);

    let payload = vec![0xABu8; 64 * 1024]; // 64 KB
    write_frame(&mut client, &payload).await;

    let received = read_frame(&mut server).await;
    assert_eq!(received.len(), 64 * 1024);
    assert_eq!(received[0], 0xAB);
}

#[tokio::test]
async fn test_framing_multiple_messages() {
    let (mut client, mut server) = tokio::io::duplex(4096);

    write_frame(&mut client, b"first").await;
    write_frame(&mut client, b"second").await;
    write_frame(&mut client, b"third").await;

    assert_eq!(read_frame(&mut server).await, b"first");
    assert_eq!(read_frame(&mut server).await, b"second");
    assert_eq!(read_frame(&mut server).await, b"third");
}

// ---------------------------------------------------------------------------
// Connection pool: owned guard for spawned tasks
// ---------------------------------------------------------------------------

#[test]
fn test_owned_guard_acquire_and_release() {
    let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
        max_connections: 2,
    }));

    let g1 = pool.try_acquire_owned();
    assert!(g1.is_some());
    assert_eq!(pool.active_count(), 1);

    let g2 = pool.try_acquire_owned();
    assert!(g2.is_some());
    assert_eq!(pool.active_count(), 2);

    drop(g1);
    assert_eq!(pool.active_count(), 1);

    drop(g2);
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn test_owned_guard_rejects_at_limit() {
    let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
        max_connections: 1,
    }));

    let _g = pool.try_acquire_owned();
    assert!(pool.try_acquire_owned().is_none());
}

// ---------------------------------------------------------------------------
// Platform-specific: full server round-trip
// ---------------------------------------------------------------------------

/// Unique pipe name per test to avoid collisions.
#[cfg(windows)]
fn unique_pipe_name(label: &str) -> String {
    let id = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(r"\\.\pipe\veritas-test-{}-{}-{}", label, id, ts)
}

#[cfg(windows)]
mod windows_server_tests {
    use super::*;
    use tokio::net::windows::named_pipe::ClientOptions;

    /// Spin up a real IPC server and do a health check round-trip.
    #[tokio::test]
    async fn test_server_health_roundtrip() {
        let pipe = unique_pipe_name("health-rt");
        let handler = test_handler();
        let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
            max_connections: 4,
        }));

        let (tx, rx) = tokio::sync::watch::channel(false);

        let server_pipe = pipe.clone();
        let server_handler = Arc::clone(&handler);
        let server_pool = Arc::clone(&pool);
        let server = tokio::spawn(async move {
            veritas_sdr::ipc::server::run_server(
                server_pipe, server_handler, server_pool, rx,
            )
            .await
        });

        // Give server time to bind
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client
        let mut client = ClientOptions::new().open(&pipe).unwrap();

        // Send health check request
        let request = br#"{"type":"health_check","check_type":"Liveness"}"#;
        write_frame(&mut client, request).await;

        // Read response
        let response = read_frame(&mut client).await;
        let text = String::from_utf8_lossy(&response);
        assert!(text.contains("health_response"), "Got: {}", text);

        // Shutdown
        let _ = tx.send(true);
        drop(client);
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }

    /// Multiple sequential requests on one connection.
    #[tokio::test]
    async fn test_server_multiple_requests() {
        let pipe = unique_pipe_name("multi-req");
        let handler = test_handler();
        let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
            max_connections: 4,
        }));

        let (tx, rx) = tokio::sync::watch::channel(false);
        let sp = pipe.clone();
        let sh = Arc::clone(&handler);
        let sc = Arc::clone(&pool);
        let server = tokio::spawn(async move {
            veritas_sdr::ipc::server::run_server(sp, sh, sc, rx).await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = ClientOptions::new().open(&pipe).unwrap();

        for _ in 0..3 {
            let req = br#"{"type":"health_check","check_type":"Full"}"#;
            write_frame(&mut client, req).await;
            let resp = read_frame(&mut client).await;
            let text = String::from_utf8_lossy(&resp);
            assert!(text.contains("health_response"));
        }

        let _ = tx.send(true);
        drop(client);
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }

    /// Sequential connections reuse the pipe (Windows creates one instance at a time).
    #[tokio::test]
    async fn test_server_sequential_connections() {
        let pipe = unique_pipe_name("sequential");
        let handler = test_handler();
        let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
            max_connections: 8,
        }));

        let (tx, rx) = tokio::sync::watch::channel(false);
        let sp = pipe.clone();
        let sh = Arc::clone(&handler);
        let sc = Arc::clone(&pool);
        let server = tokio::spawn(async move {
            veritas_sdr::ipc::server::run_server(sp, sh, sc, rx).await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect, request, disconnect -- repeat to prove reuse works.
        for i in 0..4u8 {
            let mut c = ClientOptions::new().open(&pipe).unwrap();
            let req = br#"{"type":"health_check","check_type":"Liveness"}"#;
            write_frame(&mut c, req).await;
            let resp = read_frame(&mut c).await;
            let text = String::from_utf8_lossy(&resp);
            assert!(text.contains("health_response"), "iter {}: {}", i, text);
            drop(c);
            // Let server loop back to create a fresh pipe instance
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }

    /// Graceful shutdown stops accepting new connections.
    #[tokio::test]
    async fn test_server_graceful_shutdown() {
        let pipe = unique_pipe_name("shutdown");
        let handler = test_handler();
        let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
            max_connections: 4,
        }));

        let (tx, rx) = tokio::sync::watch::channel(false);
        let sp = pipe.clone();
        let sh = Arc::clone(&handler);
        let sc = Arc::clone(&pool);
        let server = tokio::spawn(async move {
            veritas_sdr::ipc::server::run_server(sp, sh, sc, rx).await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Signal shutdown
        let _ = tx.send(true);

        // Server should exit cleanly
        let result = tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .expect("Server should exit within timeout");
        assert!(result.is_ok(), "Server task should complete");
        assert!(result.unwrap().is_ok(), "Server should return Ok");
    }

    /// Client disconnect is handled without server crash.
    #[tokio::test]
    async fn test_server_client_disconnect() {
        let pipe = unique_pipe_name("disconnect");
        let handler = test_handler();
        let pool = Arc::new(ConnectionPool::new(ConnectionConfig {
            max_connections: 4,
        }));

        let (tx, rx) = tokio::sync::watch::channel(false);
        let sp = pipe.clone();
        let sh = Arc::clone(&handler);
        let sc = Arc::clone(&pool);
        let server = tokio::spawn(async move {
            veritas_sdr::ipc::server::run_server(sp, sh, sc, rx).await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect and immediately drop (disconnect)
        {
            let _client = ClientOptions::new().open(&pipe).unwrap();
        }
        // Give server time to handle the disconnect
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Server should still be running - verify by connecting again
        let mut client = ClientOptions::new().open(&pipe).unwrap();
        let req = br#"{"type":"health_check","check_type":"Liveness"}"#;
        write_frame(&mut client, req).await;
        let resp = read_frame(&mut client).await;
        assert!(!resp.is_empty());

        let _ = tx.send(true);
        drop(client);
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }
}

// ---------------------------------------------------------------------------
// ServerError variant tests
// ---------------------------------------------------------------------------

#[test]
fn test_server_error_io_display() {
    use veritas_sdr::ipc::server::ServerError;
    let err = ServerError::Io(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "in use",
    ));
    assert!(err.to_string().contains("in use"));
}

#[test]
fn test_server_error_frame_too_large_display() {
    use veritas_sdr::ipc::server::ServerError;
    let err = ServerError::FrameTooLarge {
        size: 100,
        max: 50,
    };
    let msg = err.to_string();
    assert!(msg.contains("100"));
    assert!(msg.contains("50"));
}
