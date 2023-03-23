// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod rpc_command_processor;

use anyhow::Result;
use async_trait::async_trait;
use core::default::Default;
use std::time::Duration;

use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub use rpc_command_processor::RpcCommandProcessor;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::SuiKeyPair;

#[derive(Clone, Default)]
pub struct Payload {
    pub commands: Vec<Command>,
    /// base64 encoded keypair
    pub encoded_keypair: Option<String>,
    // TODO(chris): we should be able to derive this from the keypair?
    pub signer_address: Option<SuiAddress>,
    /// Different thread should use different gas_payment to avoid equivocation
    pub gas_payment: Option<ObjectID>,
}

#[derive(Default, Clone)]
pub struct Command {
    pub data: CommandData,
    /// 0 means the command will be run once. Default to be 0
    pub repeat_n_times: usize,
    /// how long to wait between the start of two subsequent repeats
    /// If the previous command takes longer than `repeat_interval` to finish, the next command
    /// will run as soon as the previous command finishes
    /// Default to be 0
    pub repeat_interval: Duration,
}

impl Command {
    pub fn new_dry_run() -> Self {
        Self {
            data: CommandData::DryRun(DryRun {}),
            ..Default::default()
        }
    }

    pub fn new_pay_sui() -> Self {
        Self {
            data: CommandData::PaySui(PaySui {}),
            ..Default::default()
        }
    }

    pub fn new_get_checkpoints(
        start: CheckpointSequenceNumber,
        end: Option<CheckpointSequenceNumber>,
    ) -> Self {
        Self {
            data: CommandData::GetCheckpoints(GetCheckpoints { start, end }),
            ..Default::default()
        }
    }

    pub fn with_repeat_n_times(mut self, num: usize) -> Self {
        self.repeat_n_times = num;
        self
    }

    pub fn with_repeat_interval(mut self, duration: Duration) -> Self {
        self.repeat_interval = duration;
        self
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum CommandData {
    DryRun(DryRun),
    GetCheckpoints(GetCheckpoints),
    MultiGetTransactions(MultiGetTransactions),
    PaySui(PaySui),
}

impl Default for CommandData {
    fn default() -> Self {
        CommandData::DryRun(DryRun {})
    }
}

#[derive(Clone)]
pub struct DryRun {}

#[derive(Clone, Default)]
pub struct GetCheckpoints {
    /// Default to start from 0
    start: CheckpointSequenceNumber,
    /// If None, use `getLatestCheckpointSequenceNumber`
    end: Option<CheckpointSequenceNumber>,
}

#[derive(Clone)]
pub struct MultiGetTransactions {}

#[derive(Clone)]
pub struct PaySui {}

#[async_trait]
pub trait Processor {
    /// process commands in order
    async fn apply(&self, payload: &Payload) -> Result<()>;
}

/// all payload should implement this trait
#[async_trait]
pub trait ProcessPayload<'a, T> {
    // TODO: replace SuiKeyPair with signerInfo
    async fn process(&'a self, op: T, keypair: &Option<SuiKeyPair>) -> Result<()>;
}
