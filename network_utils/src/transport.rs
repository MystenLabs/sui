// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use futures::future;
use std::io::ErrorKind;
use std::{collections::HashMap, convert::TryInto, io, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpSocket;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
};
use tracing::*;

#[cfg(test)]
#[path = "unit_tests/transport_tests.rs"]
mod transport_tests;

/// Suggested buffer size
pub const DEFAULT_MAX_DATAGRAM_SIZE: &str = "65507";

/// The handler required to create a service.
pub trait MessageHandler {
    fn handle_message<'a>(&'a self, buffer: &'a [u8]) -> future::BoxFuture<'a, Option<Vec<u8>>>;
}

/// The result of spawning a server is oneshot channel to kill it and a handle to track completion.
pub struct SpawnedServer {
    complete: futures::channel::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl SpawnedServer {
    pub async fn join(self) -> Result<(), std::io::Error> {
        // Note that dropping `self.complete` would terminate the server.
        self.handle.await??;
        Ok(())
    }

    pub async fn kill(self) -> Result<(), std::io::Error> {
        self.complete.send(()).unwrap();
        self.handle.await??;
        Ok(())
    }
}

/// Create a DataStream for this protocol.
pub async fn connect(
    address: String,
    max_data_size: usize,
) -> Result<TcpDataStream, std::io::Error> {
    TcpDataStream::connect(address, max_data_size).await
}

/// Create a DataStreamPool for this protocol.
pub async fn make_outgoing_connection_pool() -> Result<TcpDataStreamPool, std::io::Error> {
    TcpDataStreamPool::new().await
}

/// Run a server for this protocol and the given message handler.
pub async fn spawn_server<S>(
    address: &str,
    state: S,
    buffer_size: usize,
) -> Result<SpawnedServer, std::io::Error>
where
    S: MessageHandler + Send + Sync + 'static,
{
    let (complete, receiver) = futures::channel::oneshot::channel();
    let handle = {
        // see https://fly.io/blog/the-tokio-1-x-upgrade/#tcplistener-from_std-needs-to-be-set-to-nonblocking
        let std_listener = std::net::TcpListener::bind(address)?;
        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;

        tokio::spawn(run_tcp_server(listener, state, receiver, buffer_size))
    };
    Ok(SpawnedServer { complete, handle })
}

/// An implementation of DataStream based on TCP.
pub struct TcpDataStream {
    stream: TcpStream,
    max_data_size: usize,
}

impl TcpDataStream {
    async fn connect(address: String, max_data_size: usize) -> Result<Self, std::io::Error> {
        let addr = address
            .parse()
            .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;
        let socket = TcpSocket::new_v4()?;
        socket.set_send_buffer_size(max_data_size as u32)?;
        socket.set_recv_buffer_size(max_data_size as u32)?;

        let stream = socket.connect(addr).await?;
        Ok(Self {
            stream,
            max_data_size,
        })
    }

    async fn tcp_write_data<S>(stream: &mut S, buffer: &[u8]) -> Result<(), std::io::Error>
    where
        S: AsyncWrite + Unpin,
    {
        stream
            .write_all(&u32::to_le_bytes(
                buffer
                    .len()
                    .try_into()
                    .expect("length must not exceed u32::MAX"),
            ))
            .await?;
        stream.write_all(buffer).await
    }

    async fn tcp_read_data<S>(stream: &mut S, max_size: usize) -> Result<Vec<u8>, std::io::Error>
    where
        S: AsyncRead + Unpin,
    {
        let mut size_buf = [0u8; 4];
        stream.read_exact(&mut size_buf).await?;
        let size = u32::from_le_bytes(size_buf);
        if size as usize > max_size {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Message size exceeds buffer size",
            ));
        }
        let mut buf = vec![0u8; size as usize];
        stream.read_exact(&mut buf).await?;
        Ok(buf)
    }
}

impl TcpDataStream {
    pub async fn write_data<'a>(&'a mut self, buffer: &'a [u8]) -> Result<(), std::io::Error> {
        Self::tcp_write_data(&mut self.stream, buffer).await
    }

    pub async fn read_data(&mut self) -> Result<Vec<u8>, std::io::Error> {
        Self::tcp_read_data(&mut self.stream, self.max_data_size).await
    }
}

/// An implementation of DataStreamPool based on TCP.
pub struct TcpDataStreamPool {
    streams: HashMap<String, TcpStream>,
}

impl TcpDataStreamPool {
    async fn new() -> Result<Self, std::io::Error> {
        let streams = HashMap::new();
        Ok(Self { streams })
    }

    async fn get_stream(&mut self, address: &str) -> Result<&mut TcpStream, io::Error> {
        if !self.streams.contains_key(address) {
            match TcpStream::connect(address).await {
                Ok(s) => {
                    self.streams.insert(address.to_string(), s);
                }
                Err(error) => {
                    error!("Failed to open connection to {}: {}", address, error);
                    return Err(error);
                }
            };
        };
        Ok(self.streams.get_mut(address).unwrap())
    }
}

impl TcpDataStreamPool {
    pub async fn send_data_to<'a>(
        &'a mut self,
        buffer: &'a [u8],
        address: &'a str,
    ) -> Result<(), std::io::Error> {
        let stream = self.get_stream(address).await?;
        TcpDataStream::tcp_write_data(stream, buffer).await
    }
}

// Server implementation for TCP.
async fn run_tcp_server<S>(
    listener: TcpListener,
    state: S,
    mut exit_future: futures::channel::oneshot::Receiver<()>,
    buffer_size: usize,
) -> Result<(), std::io::Error>
where
    S: MessageHandler + Send + Sync + 'static,
{
    let guarded_state = Arc::new(Box::new(state));
    loop {
        let (mut stream, _) = match future::select(exit_future, Box::pin(listener.accept())).await {
            future::Either::Left(_) => break,
            future::Either::Right((value, new_exit_future)) => {
                exit_future = new_exit_future;
                value?
            }
        };

        let guarded_state = guarded_state.clone();
        tokio::spawn(async move {
            loop {
                let buffer = match TcpDataStream::tcp_read_data(&mut stream, buffer_size).await {
                    Ok(buffer) => buffer,
                    Err(err) => {
                        // We expect an EOF error at the end.
                        if err.kind() != io::ErrorKind::UnexpectedEof {
                            error!("Error while reading TCP stream: {}", err);
                        }
                        break;
                    }
                };

                if let Some(reply) = guarded_state.handle_message(&buffer[..]).await {
                    let status = TcpDataStream::tcp_write_data(&mut stream, &reply[..]).await;
                    if let Err(error) = status {
                        error!("Failed to send query response: {}", error);
                    }
                };
            }
        });
    }
    Ok(())
}
