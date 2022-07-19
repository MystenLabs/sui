# sui-swarm

This crate contains a collection of utilities for managing complete Sui
networks. The intended use for these utilities is for performing end-to-end
testing and benchmarking. In the future, the expectation is that we'll have
support for a number of different "backends" for how the network is operated.
Today the only supplied backend is the `memory` backend, although in the future
we should be able to support a multi-process and even a Kubernetes (k8s) backend.

## Backends

### memory

An `in-memory`, or rather `in-process`, backend for building and managing Sui
networks that all run inside the same process. Nodes are isolated from one
another by each being run on their own separate thread within their own `tokio`
runtime. This enables the ability to properly shut down a single node and
ensure that all of its running tasks are also shut down, something that is
extremely difficult or down right impossible to do if all the nodes are running
on the same runtime.
