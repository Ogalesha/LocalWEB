//! Per-project loopback TCP proxies for shared database engines.

use std::net::{Ipv4Addr, SocketAddr};

use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::{JoinHandle, JoinSet},
};
use tracing::{info, warn};

/// A running database proxy that stops when dropped through [`stop`](DbProxy::stop).
pub struct DbProxy {
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl DbProxy {
    /// Whether the owned listener task can still accept connections.
    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.task.is_finished()
    }
    /// Binds `listen_port` on loopback and forwards connections to `target_port`.
    ///
    /// # Errors
    ///
    /// Returns an error when the listen port is already occupied or cannot be bound.
    pub async fn start(listen_port: u16, target_port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, listen_port)).await?;
        let (shutdown, mut receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut receiver => break,
                    _ = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((mut client, _)) => {
                                connections.spawn(async move {
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
        let _ = (&mut self.task).await;
    }
}

impl Drop for DbProxy {
    fn drop(&mut self) {
        // Aborting the listener also drops its JoinSet and closes every
        // established connection, including when startup is rolled back.
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn listener_exit_invalidates_the_running_proxy() {
        let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let mut proxy = DbProxy::start(port, 9).await.unwrap();
        assert!(proxy.is_running());
        proxy.task.abort();
        (&mut proxy.task).await.unwrap_err();
        assert!(!proxy.is_running());
        let _rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejects_a_listen_port_already_in_use() {
        let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let error = DbProxy::start(port, 9).await.err().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    }

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

    #[tokio::test]
    async fn stop_closes_established_connections_and_releases_the_port() {
        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let proxy = DbProxy::start(port, upstream.local_addr().unwrap().port())
            .await
            .unwrap();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let (mut server, _) = upstream.accept().await.unwrap();
        client.write_all(b"x").await.unwrap();
        let mut byte = [0];
        server.read_exact(&mut byte).await.unwrap();
        proxy.stop().await;
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), server.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        let _rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
    }
}
