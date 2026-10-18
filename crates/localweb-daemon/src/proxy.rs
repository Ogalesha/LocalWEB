//! Per-project loopback TCP proxies for shared database engines.

use std::net::{Ipv4Addr, SocketAddr};

use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};
use tracing::{info, warn};

/// A running database proxy that stops when dropped through [`stop`](DbProxy::stop).
pub struct DbProxy {
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl DbProxy {
    /// Binds `listen_port` on loopback and forwards connections to `target_port`.
    ///
    /// # Errors
    ///
    /// Returns an error when the listen port is already occupied or cannot be bound.
    pub async fn start(listen_port: u16, target_port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, listen_port)).await?;
        let (shutdown, mut receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut receiver => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((mut client, _)) => {
                                tokio::spawn(async move {
                                    let target = SocketAddr::from((Ipv4Addr::LOCALHOST, target_port));
                                    match TcpStream::connect(target).await {
                                        Ok(mut upstream) => {
                                            let _ = copy_bidirectional(&mut client, &mut upstream).await;
                                        }
                                        Err(error) => {
                                            warn!(%error, port = listen_port, "database proxy upstream unavailable");
                                        }
                                    }
                                });
                            }
                            Err(error) => {
                                warn!(%error, port = listen_port, "database proxy listener failed");
                                break;
                            }
                        }
                    }
                }
            }
            info!(port = listen_port, "database proxy stopped");
        });
        Ok(Self {
            shutdown: Some(shutdown),
            task,
        })
    }

    /// Stops accepting connections and waits for the listener task.
    pub async fn stop(mut self) {
        drop(self.shutdown.take());
        let _ = self.task.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn forwards_bytes_between_client_and_upstream() {
        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let target_port = upstream.local_addr().unwrap().port();
        let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let listen_port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let proxy = DbProxy::start(listen_port, target_port).await.unwrap();
        let echo = tokio::spawn(async move {
            let (mut socket, _) = upstream.accept().await.unwrap();
            let mut buffer = [0u8; 4];
            socket.read_exact(&mut buffer).await.unwrap();
            socket.write_all(&buffer).await.unwrap();
        });
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, listen_port))
            .await
            .unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut buffer = [0u8; 4];
        client.read_exact(&mut buffer).await.unwrap();
        assert_eq!(&buffer, b"ping");
        echo.await.unwrap();
        proxy.stop().await;
    }
}
