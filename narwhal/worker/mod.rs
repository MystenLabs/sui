mod consensus;

pub use consensus::consensus_api_client::*;
pub use consensus::consensus_api_server::*;
pub use consensus::*;
pub(crate) static CONSENSUS_LISTENER: Lazy<Arc<ConsensusListener>> =
    Lazy::new(|| Arc::new(ConsensusListener::default()));
