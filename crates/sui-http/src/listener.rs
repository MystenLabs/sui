// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

/// Types that can listen for connections.
pub trait Listener: Send + 'static {
    /// The listener's IO type.
    type Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static;

    /// The listener's address type.
    // all these bounds are necessary to add this information in a request extension
    type Addr: Clone + Send + Sync + 'static;

    /// Accept a new incoming connection to this listener.
    ///
    /// If the underlying accept call can return an error, this function must
    /// take care of logging and retrying.
    fn accept(&mut self) -> impl std::future::Future<Output = (Self::Io, Self::Addr)> + Send;

    /// Returns the local address that this listener is bound to.
    fn local_addr(&self) -> std::io::Result<Self::Addr>;
}

/// Extensions to [`Listener`].
pub trait ListenerExt: Listener + Sized {
    /// Run a mutable closure on every accepted `Io`.
    ///
    /// # Example
    ///
    /// ```
    /// use tracing::trace;
    /// use sui_http::ListenerExt;
    ///
    /// # async {
    /// let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
    ///     .await
    ///     .unwrap()
    ///     .tap_io(|tcp_stream| {
    ///         if let Err(err) = tcp_stream.set_nodelay(true) {
    ///             trace!("failed to set TCP_NODELAY on incoming connection: {err:#}");
    ///         }
    ///     });
    /// # };
    /// ```
    fn tap_io<F>(self, tap_fn: F) -> TapIo<Self, F>
    where
        F: FnMut(&mut Self::Io) + Send + 'static,
    {
        TapIo {
            listener: self,
            tap_fn,
        }
    }
}

impl<L: Listener> ListenerExt for L {}

impl Listener for tokio::net::TcpListener {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match Self::accept(self).await {
                Ok(tup) => return tup,
                Err(e) => handle_accept_error(e).await,
            }
        }
    }

    #[inline]
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Self::local_addr(self)
    }
}

#[derive(Debug)]
pub struct TcpListenerWithOptions {
    inner: tokio::net::TcpListener,
    nodelay: bool,
    keepalive: Option<Duration>,
}

impl TcpListenerWithOptions {
    pub fn new<A: std::net::ToSocketAddrs>(
        addr: A,
        nodelay: bool,
        keepalive: Option<Duration>,
    ) -> Result<Self, crate::BoxError> {
        let std_listener = std::net::TcpListener::bind(addr)?;
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        Ok(Self::from_listener(listener, nodelay, keepalive))
    }

    /// Creates a new `TcpIncoming` from an existing `tokio::net::TcpListener`.
    pub fn from_listener(
        listener: tokio::net::TcpListener,
        nodelay: bool,
        keepalive: Option<Duration>,
    ) -> Self {
        Self {
            inner: listener,
            nodelay,
            keepalive,
        }
    }

    // Consistent with hyper-0.14, this function does not return an error.
    fn set_accepted_socket_options(&self, stream: &tokio::net::TcpStream) {
        if self.nodelay {
            if let Err(e) = stream.set_nodelay(true) {
                tracing::warn!("error trying to set TCP nodelay: {}", e);
            }
        }

        if let Some(timeout) = self.keepalive {
            let sock_ref = socket2::SockRef::from(&stream);
            let sock_keepalive = socket2::TcpKeepalive::new().with_time(timeout);

            if let Err(e) = sock_ref.set_tcp_keepalive(&sock_keepalive) {
                tracing::warn!("error trying to set TCP keepalive: {}", e);
            }
        }
    }
}

impl Listener for TcpListenerWithOptions {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (io, addr) = Listener::accept(&mut self.inner).await;
        self.set_accepted_socket_options(&io);
        (io, addr)
    }

    #[inline]
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Listener::local_addr(&self.inner)
    }
}

// Uncomment once we update tokio to >=1.41.0
// #[cfg(unix)]
// impl Listener for tokio::net::UnixListener {
//     type Io = tokio::net::UnixStream;
//     type Addr = std::os::unix::net::SocketAddr;

//     async fn accept(&mut self) -> (Self::Io, Self::Addr) {
//         loop {
//             match Self::accept(self).await {
//                 Ok((io, addr)) => return (io, addr.into()),
//                 Err(e) => handle_accept_error(e).await,
//             }
//         }
//     }

//     #[inline]
//     fn local_addr(&self) -> std::io::Result<Self::Addr> {
//         Self::local_addr(self).map(Into::into)
//     }
// }

/// Return type of [`ListenerExt::tap_io`].
///
/// See that method for details.
pub struct TapIo<L, F> {
    listener: L,
    tap_fn: F,
}

impl<L, F> std::fmt::Debug for TapIo<L, F>
where
    L: Listener + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TapIo")
            .field("listener", &self.listener)
            .finish_non_exhaustive()
    }
}

impl<L, F> Listener for TapIo<L, F>
where
    L: Listener,
    F: FnMut(&mut L::Io) + Send + 'static,
{
    type Io = L::Io;
    type Addr = L::Addr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (mut io, addr) = self.listener.accept().await;
        (self.tap_fn)(&mut io);
        (io, addr)
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

async fn handle_accept_error(e: std::io::Error) {
    if is_connection_error(&e) {
        return;
    }

    // [From `hyper::Server` in 0.14](https://github.com/hyperium/hyper/blob/v0.14.27/src/server/tcp.rs#L186)
    //
    // > A possible scenario is that the process has hit the max open files
    // > allowed, and so trying to accept a new connection will fail with
    // > `EMFILE`. In some cases, it's preferable to just wait for some time, if
    // > the application will likely close some files (or connections), and try
    // > to accept the connection again. If this option is `true`, the error
    // > will be logged at the `error` level, since it is still a big deal,
    // > and then the listener will sleep for 1 second.
    //
    // hyper allowed customizing this but axum does not.
    tracing::error!("accept error: {e}");
    tokio::time::sleep(Duration::from_secs(1)).await;
}

fn is_connection_error(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;

    matches!(
        e.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::BrokenPipe
            | ErrorKind::Interrupted
            | ErrorKind::WouldBlock
            | ErrorKind::TimedOut
    )
}
