// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::server::TlsStream;

pub(crate) enum ServerIo<IO> {
    Io(IO),
    TlsIo(Box<TlsStream<IO>>),
}

impl<IO> ServerIo<IO> {
    pub(crate) fn new_io(io: IO) -> Self {
        Self::Io(io)
    }

    pub(crate) fn new_tls_io(io: TlsStream<IO>) -> Self {
        Self::TlsIo(Box::new(io))
    }

    pub(crate) fn peer_certs(
        &self,
    ) -> Option<std::sync::Arc<Vec<tokio_rustls::rustls::pki_types::CertificateDer<'static>>>> {
        match self {
            Self::Io(_) => None,
            Self::TlsIo(io) => {
                let (_inner, session) = io.get_ref();

                session
                    .peer_certificates()
                    .map(|certs| certs.to_owned().into())
            }
        }
    }
}

impl<IO> AsyncRead for ServerIo<IO>
where
    IO: AsyncWrite + AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Io(io) => Pin::new(io).poll_read(cx, buf),
            Self::TlsIo(io) => Pin::new(io).poll_read(cx, buf),
        }
    }
}

impl<IO> AsyncWrite for ServerIo<IO>
where
    IO: AsyncWrite + AsyncRead + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            Self::Io(io) => Pin::new(io).poll_write(cx, buf),
            Self::TlsIo(io) => Pin::new(io).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Io(io) => Pin::new(io).poll_flush(cx),
            Self::TlsIo(io) => Pin::new(io).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Io(io) => Pin::new(io).poll_shutdown(cx),
            Self::TlsIo(io) => Pin::new(io).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match &mut *self {
            Self::Io(io) => Pin::new(io).poll_write_vectored(cx, bufs),
            Self::TlsIo(io) => Pin::new(io).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Io(io) => io.is_write_vectored(),
            Self::TlsIo(io) => io.is_write_vectored(),
        }
    }
}
