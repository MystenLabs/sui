# Roadmap

Currently, builders can:
* Write and test Move smart contracts,
* Spin up a local Sui network,
* Publish and run Move smart contracts on a local network.

In the coming months, we will release:
1. A public devnet that allows Sui devs to do all of the above on a shared network powered by Mysten-operated authorities
2. A public testnet that onboards a diverse set of non-Mysten authorities to the network
3. A public mainnet with real assets and production applications!

A more fine-grained description of the upcoming features and improvements to the Sui codebase follows.

# Ongoing Work and Upcoming Features

### Internal Devnet
* Constantly running Sui network
* Productionizing network stack
* Benchmarking throughput and latency in various configurations

### Protocol Stabilization

* Implementing reconfiguration and staking
* Ledger and state checkpoints
* Aligning Gateway Service Rust/REST/wallet API’s
* Finalizing REST data model (aka SuiJSON)
* Finalizing core data types
* Authority "Follower" API's to support replicas
* Integration of shared objects and consensus
* Selecting principled gas costs
* Event indexing hints

### SDK and Ecosystem
* Block explorer
* Key management and wallet prototypes
* Support package publishing via REST API
* More informative error messages
* More convenient API's that hide gas object selection and nested object authentication

### Move Development Improvments
* Allow objects used in authentication, but not passed to entrypoints
* Explicit syntax and compiler enforcement for entrypoints
* Adding Move Prover specs to the Sui framework and verifying in CI
