// Copyright (c) 2024, Scalaris.
// SPDX-License-Identifier: Apache-2.0

mod consensus_output_api;
mod consensus_result;
mod consensus_service;
mod types;

use std::sync::Arc;

pub(crate) use consensus_output_api::ConsensusOutputAPI;
pub use consensus_result::ConsensusListener;
pub use consensus_service::ConsensusService;
use once_cell::sync::Lazy;
pub use types::{ConsensusTransactionWrapper, NsTransaction};

/*
 *
 * Create static variable for listen consensus ouput
 */
pub(crate) static CONSENSUS_LISTENER: Lazy<Arc<ConsensusListener>> =
    Lazy::new(|| Arc::new(ConsensusListener::default()));
