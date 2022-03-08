// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{Sink, SinkExt, Stream, StreamExt};
use std::io::ErrorKind;
use std::sync::Arc;
use tokio::net::TcpSocket;
use tokio::net::{TcpListener, TcpStream};

use async_trait::async_trait;

use tracing::*;

use bytes::{Bytes, BytesMut};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[cfg(test)]
#[path = "unit_tests/transport_tests.rs"]
mod transport_tests;

/// Suggested buffer size
pub const DEFAULT_MAX_DATAGRAM_SIZE: usize = 65507;
pub const DEFAULT_MAX_DATAGRAM_SIZE_STR: &str = "65507";

/// The handler required to create a service.
#[async_trait]
pub trait MessageHandler<A> {
    async fn handle_messages(&self, channel: A) -> ();
}

/*
    The RwChannel connects the low-level networking code here, that handles
    TCP streams, ports, accept/connect, and sockets that provide AsyncRead /
    AsyncWrite on byte streams, with the higher level logic in AuthorityServer
    that handles sequences of Bytes / BytesMut, as framed messages, through
    exposing a standard Stream and Sink trait.

    This separation allows us to change the details of the network, transport
    and framing, without changing the authority code. It also allows us to test
    the authority without using a real network.
*/
pub trait RwChannel<'a> {
    type R: 'a + Stream<Item = Result<BytesMut, std::io::Error>> + Unpin + Send;
    type W: 'a + Sink<Bytes, Error = std::io::Error> + Unpin + Send;

    fn sink(&mut self) -> &mut Self::W;
    fn stream(&mut self) -> &mut Self::R;
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

/// Run a server for this protocol and the given message handler.
pub async fn spawn_server<S>(
    address: &str,
    state: S,
    buffer_size: usize,
) -> Result<SpawnedServer, std::io::Error>
where
    S: MessageHandler<TcpDataStream> + Send + Sync + 'static,
{
    let (complete, receiver) = futures::channel::oneshot::channel();
    let handle = {
        // see https://fly.io/blog/the-tokio-1-x-upgrade/#tcplistener-from_std-needs-to-be-set-to-nonblocking
        let std_listener = std::net::TcpListener::bind(address)?;

        if let Ok(local_addr) = std_listener.local_addr() {
            let host = local_addr.ip();
            let port = local_addr.port();
            info!("Listening to TCP traffic on {host}:{port}");
        }

        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;

        tokio::spawn(run_tcp_server(listener, state, receiver, buffer_size))
    };
    Ok(SpawnedServer { complete, handle })
}

/// An implementation of DataStream based on TCP.
pub struct TcpDataStream {
    framed: Framed<TcpStream, LengthDelimitedCodec>,
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
        Ok(TcpDataStream::from_tcp_stream(stream, max_data_size))
    }

    fn from_tcp_stream(stream: TcpStream, max_data_size: usize) -> TcpDataStream {
        let framed = Framed::new(
            stream,
            LengthDelimitedCodec::builder()
                .max_frame_length(max_data_size)
                .new_codec(),
        );

        Self { framed }
    }

    // TODO: Eliminate vecs and use Byte, ByteBuf

    pub async fn write_data<'a>(&'a mut self, buffer: &'a [u8]) -> Result<(), std::io::Error> {
        self.framed.send(buffer.to_vec().into()).await
    }

    pub async fn read_data(&mut self) -> Option<Result<Vec<u8>, std::io::Error>> {
        let result = self.framed.next().await;
        result.map(|v| v.map(|w| w.to_vec()))
    }
}

impl<'a> RwChannel<'a> for TcpDataStream {
    type W = Framed<TcpStream, LengthDelimitedCodec>;
    type R = Framed<TcpStream, LengthDelimitedCodec>;

    fn sink(&mut self) -> &mut Self::W {
        &mut self.framed
    }
    fn stream(&mut self) -> &mut Self::R {
        &mut self.framed
    }
}

// Server implementation for TCP.
async fn run_tcp_server<S>(
    listener: TcpListener,
    state: S,
    mut exit_future: futures::channel::oneshot::Receiver<()>,
    _buffer_size: usize,
) -> Result<(), std::io::Error>
where
    S: MessageHandler<TcpDataStream> + Send + Sync + 'static,
{
    let guarded_state = Arc::new(state);
    loop {
        let stream;

        tokio::select! {
            _ = &mut exit_future => { break },
            result = listener.accept() => {
                let (value, _addr) = result?;
                stream = value;
            }
        }

        let guarded_state = guarded_state.clone();
        tokio::spawn(async move {
            let framed = TcpDataStream::from_tcp_stream(stream, _buffer_size);
            guarded_state.handle_messages(framed).await
        });
    }
    Ok(())
}
