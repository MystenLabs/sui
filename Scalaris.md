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

3. docker/sui-node/Dockerfile 

Add protobuf-compiler

```
RUN apt-get update && apt-get install -y cmake clang protobuf-compiler

```

4. docker/sui-network/docker-compose-scalaris.yaml

Build scalaris docker image
```
docker/sui-node/build.sh -t scalaris/consensus-node
```

Build local stress image 

```
cd docker/stress
docker build -t stress:testing --build-arg SUI_TOOLS_IMAGE_TAG=mainnet-v1.19.1 .
```

Build genesic files
```
cd docker/sui-network
docker build --file Dockerfile --output "type=local,dest=./"
```
Run containers in folder docker/sui-network

```
docker-compose -f docker-compose-scalaris.yaml up -d
```