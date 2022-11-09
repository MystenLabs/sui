// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate component;

use async_trait::async_trait;
use component::{IrrecoverableError, Manageable, Supervisor};
use eyre::eyre;
use std::cmp::min;
use std::sync::Once;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;
use tokio::task::JoinHandle;

static mut SHOULD_FAIL: bool = true;
static FIXER: Once = Once::new();

fn fix() {
    FIXER.call_once(|| unsafe {
        SHOULD_FAIL = false;
    })
}

/// We create two structs, an empty struct as a component that will contain functions
/// that create and fully encapsulate an instance of the actual type.
/// An instance of the actual type needs to be instantiated inside the supervised task
/// in order to have the correct lifetime.
pub struct MockTcpStreamComponent {}

pub struct MockTcpStream {
    read_data: Vec<u8>,
}

impl MockTcpStream {
    pub fn new() -> Self {
        let read_data = Vec::new();
        MockTcpStream { read_data }
    }

    /// This function will fail on the first call and then succeed on preceding calls to create
    /// a situation where we have an irrecoverable error.
    fn mock_read(&self, buf: &mut [u8]) -> Result<usize, eyre::Report> {
        // failure should happen once
        unsafe {
            if SHOULD_FAIL {
                fix();
                return Result::Err(eyre!("Could not read from stream."));
            }
        }

        let size: usize = min(self.read_data.len(), buf.len());
        buf[..size].copy_from_slice(&self.read_data[..size]);
        Ok(size)
    }
}

impl Default for MockTcpStream {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTcpStreamComponent {
    /// This is a function that should run continuously doing some operation, here we are
    /// continuously listening on a mocked TCP Stream. This is ultimately the function that we
    /// are supervising.
    /// Inside this function we first initialize any state that will be used in this component so
    /// that the scope is also correctly reset on a restart.
    ///
    /// This would be an excellent place to also add a scopeguard with a defer_panic so that if the
    /// component panics without a user caught irrecoverable error, a descriptive error message and/
    /// or stacktrace can be forwarded to the supervisor.
    pub async fn listen(
        tx_irrecoverable: Sender<eyre::Report>,
        rx_cancellation: oneshotReceiver<()>,
    ) {
        // Initialize the concrete type
        let m_tcp = MockTcpStream::new();

        loop {
            let mut buf = [0; 10];
            match m_tcp.mock_read(&mut buf) {
                Ok(_) => {} // process
                Err(_) => {
                    let e = eyre!("missing something required");
                    tx_irrecoverable
                        .send(e)
                        .await
                        .expect("Could not send irrecoverable signal.");
                    wait_for_cancellation(rx_cancellation).await;
                    return;
                }
            };
        }
    }
}

/// Wait for the cancellation signal in order to ensure that the message we sent to the
/// supervisor was received before we return which causes the join handle to complete. If we
/// were to return immediately, there would be no guarantee the message we send will be received.
async fn wait_for_cancellation(rx_cancellation: oneshotReceiver<()>) {
    loop {
        tokio::select! {
            _ = rx_cancellation => {
                println!("terminating component task");
                break;
            }
        }
    }
}

#[async_trait]
impl Manageable for MockTcpStreamComponent {
    #[allow(clippy::async_yields_async)]
    /// The start function spawns a tokio task supplied with a function that
    /// should be constantly running.
    async fn start(
        &self,
        tx_irrecoverable: Sender<eyre::Report>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        println!("starting component task");
        let handle: JoinHandle<()> = tokio::spawn(Self::listen(tx_irrecoverable, rx_cancellation));
        handle
    }

    /// Implement this function to log the error messages or take any task-specific action such as
    /// closing a file or terminating children tasks.
    fn handle_irrecoverable(
        &mut self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), eyre::Report> {
        println!("Received irrecoverable error: {irrecoverable}");
        Ok(())
    }
}

#[tokio::main]
pub async fn main() -> Result<(), eyre::Report> {
    // Create a component
    let stream_component = MockTcpStreamComponent {};

    // Create a supervisor for the component
    let supervisor = Supervisor::new(stream_component);

    // Spawn the supervisor to start the component and supervision.
    match supervisor.spawn().await {
        Ok(_) => {}
        Err(e) => println!("Got this error {:?}", e),
    };
    Ok(())
}
