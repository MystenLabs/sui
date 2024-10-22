// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{workloads::ExpectedFailureType, ExecutionEffects};
use std::fmt::Display;
use sui_types::transaction::Transaction;

/// A Payload is a transaction wrapper of a particular type (transfer object, shared counter, etc).
/// Calling `make_transaction()` on a payload produces the transaction it is wrapping. Once that
/// transaction is returned with effects (by quorum driver), a new payload can be generated with that
/// effect by invoking `make_new_payload(effects)`
pub trait Payload: Send + Sync + std::fmt::Debug + Display {
    fn make_new_payload(&mut self, effects: &ExecutionEffects);
    fn make_transaction(&mut self) -> Transaction;
    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None // Default implementation returns None
    }
}
