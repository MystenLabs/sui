# Roadmap

This document summarizes current state for the Sui blockchain and hints at impending changes. For the latest updates, see:
https://docs.sui.io/devnet/learn#see-whats-new

Currently, Sui builders can:
* Connect to Sui Devnet
* Write and test Move smart contracts
* Publish and run Move smart contracts
* Program with Sui objects
* Run a Sui Fullnode
* Use Sui Explorer to see transactions

See instructions for all of the above at:
https://docs.sui.io

In the coming months, we will release:
1. A public testnet that onboards a diverse set of non-Mysten authorities to the network
1. A public mainnet with real assets and production applications!

A more fine-grained description of the upcoming features and improvements to the Sui codebase follows.

## Ongoing work and upcoming features

### Internal Devnet
* Productionizing network stack
* Benchmarking throughput and latency in various configurations

### Protocol stabilization

* Implementing reconfiguration and staking
* Ledger and state checkpoints
* Finalizing core data types
* Authority "Follower" APIs to support replicas
* Integration of shared objects and consensus
* Selecting principled gas costs
* Event indexing hints

### SDK and ecosystem
* Key management and wallet prototypes
* More informative error messages
* More convenient APIs that hide gas object selection and nested object authentication

### Move development improvements
* Allow objects used in authentication, but not passed to entrypoints
* Explicit syntax and compiler enforcement for entrypoints
* Adding Move Prover specs to the Sui framework and verifying in continuous integration (CI)
