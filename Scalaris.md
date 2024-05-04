# Scalaris extension

1. Add grpc_consensus_service into sui_node crate by add a service layer to the server_builder
![alt text](lib.png)

2. Add scalaris logic for grpc service in the crate sui-core under the package scalaris
Modify small logic in the sui-core/src/consensus_handler.rs
Add extension module in the end of  sui-core/src/lib.rs

```
//----- Begin Scalaris extension -----//
pub mod scalaris;
// Reexport consensus-common
pub use consensus_common::proto::ConsensusApiServer;
pub use scalaris::ConsensusTransactionWrapper;
//----- End of scalaris extension -----//

```