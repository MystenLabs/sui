// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{runtime::Runtime, time::timeout};
use tracing::error;

async fn get_new_local_address() -> Result<String, std::io::Error> {
    let client = tokio::net::TcpSocket::new_v4()?;
    client.set_reuseaddr(true)?;
    client.bind("127.0.0.1:0".parse().unwrap())?;
    Ok(format!("{}", client.local_addr()?))
}

struct TestService {
    counter: Arc<AtomicUsize>,
}

impl TestService {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        TestService { counter }
    }

    async fn handle_one_message<'a>(&'a self, buffer: &'a [u8]) -> Option<Vec<u8>> {
        self.counter.fetch_add(buffer.len(), Ordering::Relaxed);
        Some(Vec::from(buffer))
    }
}

#[async_trait]
impl<'a, A> MessageHandler<A> for TestService
where
    A: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut channel: A) -> () {
        loop {
            let buffer = match channel.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(err)) => {
                    // We expect some EOF or disconnect error at the end.
                    error!("Error while reading TCP stream: {err}");
                    break;
                }
                None => {
                    break;
                }
            };

            if let Some(reply) = self.handle_one_message(&buffer[..]).await {
                let status = channel.sink().send(reply.into()).await;
                if let Err(error) = status {
                    error!("Failed to send query response: {error}");
                }
            };
        }
    }
}

async fn test_server() -> Result<(usize, usize), std::io::Error> {
    let address = get_new_local_address().await.unwrap();

    let counter = Arc::new(AtomicUsize::new(0));
    let mut received = 0;

    let server = spawn_server(&address, Arc::new(TestService::new(counter.clone())), 100).await?;

    let mut client = connect(address.clone(), 1000).await?;
    client.write_data(b"abcdef").await?;
    received += client.read_data().await.unwrap()?.len();
    client.write_data(b"abcd").await?;
    received += client.read_data().await.unwrap()?.len();

    // Try to read data on the first connection (should fail).
    received += timeout(Duration::from_millis(500), client.read_data())
        .await
        .unwrap_or_else(|_| Some(Ok(Vec::new())))
        .unwrap()?
        .len();

    // Attempt to gracefully kill server.
    server.kill().await?;

    timeout(Duration::from_millis(500), client.write_data(b"abcd"))
        .await
        .unwrap_or(Ok(()))?;
    received += timeout(Duration::from_millis(500), client.read_data())
        .await
        .unwrap_or_else(|_| Some(Ok(Vec::new())))
        .unwrap()?
        .len();

    Ok((counter.load(Ordering::Relaxed), received))
}

#[test]
fn tcp_server() {
    let rt = Runtime::new().unwrap();
    let (processed, received) = rt.block_on(test_server()).unwrap();
    // Active TCP connections are allowed to finish before the server is gracefully killed.
    assert_eq!(processed, 14);
    assert_eq!(received, 14);
}
