// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::IntGauge;

use super::DagError;
use crate::metered_channel::{channel, Receiver, Sender, WithPermit};
use std::time::Duration;

pub struct Processor {
    input: Receiver<Result<usize, DagError>>,
    output: Sender<usize>,
}

impl Processor {
    pub fn new(input: Receiver<Result<usize, DagError>>, output: Sender<usize>) -> Self {
        Self { input, output }
    }

    pub fn spawn(input: Receiver<Result<usize, DagError>>, output: Sender<usize>) {
        tokio::spawn(async move {
            let mut processor = Processor::new(input, output);
            processor.run().await;
        });
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                Some((permit, Some(res_value))) = self.output.with_permit(self.input.recv())  => {
                    permit.send(res_value.unwrap());
                }
            }
        }
    }
}

#[tokio::test]
async fn with_permit_unhappy_case() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();

    let (tx_inbound, rx_inbound) = channel(100, &counter); // we'll make sure we always have stuff inbound
    let (tx_outbound, mut rx_outbound) = channel(1, &counter); // we'll constrain the output

    Processor::spawn(rx_inbound, tx_outbound);
    // we fill the inbound channel with stuff
    (0..100).for_each(|i| {
        tx_inbound
            .try_send(Ok(i))
            .expect("failed to send to inbound channel");
    });

    tokio::time::sleep(Duration::from_secs(1)).await;
    // by now, the outbound channel should fail to deliver permits on each loop pass,
    // whereas the inbound channel is full

    // we now try to receive all the things we can from the outbound channel
    let mut recvd = vec![];
    (0..100).for_each(|_| {
        if let Ok(val) = rx_outbound.try_recv() {
            recvd.push(val);
        }
    });
    assert_eq!(recvd, (0..100).collect::<Vec<usize>>());
}
