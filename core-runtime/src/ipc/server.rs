//! IPC server loop for accepting and processing connections.
//!
//! Uses platform-specific transports:
//! - Unix: `tokio::net::UnixListener` (file-based sockets)
//! - Windows: `tokio::net::windows::named_pipe` (named pipes)
//!
//! All connections use length-prefixed framing (4-byte LE + payload)
//! matching the CLI client protocol in `cli::ipc_client`.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;
use thiserror::Error;

use super::connections::{ConnectionPool, OwnedConnectionGuard};
use super::handler::IpcHandler;

/// Maximum allowed message frame size (16 MB).
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Frame too large: {size} bytes (max {max})")]
    FrameTooLarge { size: usize, max: usize },
}

/// Read a length-prefixed frame from an async reader.
async fn read_frame<R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, ServerError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;

    let frame_len = u32::from_le_bytes(len_buf) as usize;
    if frame_len > MAX_FRAME_SIZE {
        return Err(ServerError::FrameTooLarge {
            size: frame_len,
            max: MAX_FRAME_SIZE,
        });
    }

    let mut buf = vec![0u8; frame_len];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Write a length-prefixed frame to an async writer.
async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> Result<(), ServerError> {
    let len = data.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await?;
    Ok(())
}

/// Handle one IPC connection: read requests, dispatch, write responses.
async fn handle_connection<S: AsyncReadExt + AsyncWriteExt + Unpin>(
    mut stream: S,
    handler: Arc<IpcHandler>,
    _guard: OwnedConnectionGuard,
) {
    let mut session = None;

    loop {
        let request_bytes = match read_frame(&mut stream).await {
            Ok(bytes) => bytes,
            Err(ServerError::Io(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break; // Client disconnected
            }
            Err(e) => {
                eprintln!("Connection read error: {}", e);
                break;
            }
        };

        match handler.process(&request_bytes, session.as_ref()).await {
            Ok((response_bytes, new_session)) => {
                if new_session.is_some() {
                    session = new_session;
                }
                if let Err(e) = write_frame(&mut stream, &response_bytes).await {
                    eprintln!("Connection write error: {}", e);
                    break;
                }
            }
            Err(e) => {
                let err = format!(
                    r#"{{"type":"error","code":500,"message":"{}"}}"#,
                    e
                );
                let _ = write_frame(&mut stream, err.as_bytes()).await;
                break;
            }
        }
    }
}

/// Accept one connection, acquire a guard, and spawn a handler task.
fn spawn_connection<S: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static>(
    stream: S,
    handler: &Arc<IpcHandler>,
    connections: &Arc<ConnectionPool>,
) {
    let guard = match connections.try_acquire_owned() {
        Some(g) => g,
        None => {
            eprintln!("Connection limit reached, rejecting client");
            return;
        }
    };
    let handler = Arc::clone(handler);
    tokio::spawn(async move {
        handle_connection(stream, handler, guard).await;
    });
}

/// Run the IPC server on Unix (Unix domain socket).
#[cfg(unix)]
pub async fn run_server(
    socket_path: String,
    handler: Arc<IpcHandler>,
    connections: Arc<ConnectionPool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), ServerError> {
    use tokio::net::UnixListener;

    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    eprintln!("IPC server listening on {}", socket_path);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => spawn_connection(
                        stream, &handler, &connections,
                    ),
                    Err(e) => eprintln!("Accept error: {}", e),
                }
            }
            _ = shutdown_rx.changed() => {
                eprintln!("IPC server shutting down");
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

/// Run the IPC server on Windows (named pipes).
#[cfg(windows)]
pub async fn run_server(
    pipe_name: String,
    handler: Arc<IpcHandler>,
    connections: Arc<ConnectionPool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), ServerError> {
    use tokio::net::windows::named_pipe::ServerOptions;

    eprintln!("IPC server listening on {}", pipe_name);

    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(false)
            .create(&pipe_name)?;

        tokio::select! {
            result = server.connect() => {
                match result {
                    Ok(()) => spawn_connection(
                        server, &handler, &connections,
                    ),
                    Err(e) => eprintln!("Pipe connect error: {}", e),
                }
            }
            _ = shutdown_rx.changed() => {
                eprintln!("IPC server shutting down");
                break;
            }
        }
    }

    Ok(())
}
