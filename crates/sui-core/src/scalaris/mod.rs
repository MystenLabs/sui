mod consensus_output_api;
mod consensus_result;
mod consensus_service;
mod types;

use std::sync::Arc;

pub(crate) use consensus_output_api::ConsensusOutputAPI;
pub use consensus_result::ConsensusListener;
pub use consensus_service::ConsensusService;
use lazy_static::lazy_static;
pub use types::{ConsensusTransactionWrapper, NsTransaction};

/*
 *
 * Create static variable for listen consensus ouput
 */
lazy_static! {
    pub static ref CONSENSUS_LISTENER: Arc<ConsensusListener> =
        Arc::new(ConsensusListener::default());
}
