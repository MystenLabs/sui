// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod consensus_output_api;
mod consensus_result;
mod consensus_service;
mod consensus_validator;
mod types;
use std::sync::Arc;

pub(crate) use consensus_output_api::ConsensusOutputAPI;
pub use consensus_result::ConsensusListener;
pub use consensus_service::ConsensusService;
pub use consensus_validator::{ScalarisTxValidator, ScalarisTxValidatorMetrics};
use once_cell::sync::Lazy;
pub use types::{ConsensusTransactionWrapper, NsTransaction};

pub trait TraitValidator:
    narwhal_worker::TransactionValidator + consensus_core::TransactionVerifier
{
}
/*
 *
 * Create static variable for listen consensus ouput
 */
pub(crate) static CONSENSUS_LISTENER: Lazy<Arc<ConsensusListener>> =
    Lazy::new(|| Arc::new(ConsensusListener::default()));
