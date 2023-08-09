# Running the code

```
cargo build --release --bin baseline_executor
time ./target/release/baseline_executor --config-path fullnode.yaml --download 500000 --execute 500000

cargo build --release --bin simple_channel_executor
time ./target/release/simple_channel_executor --config-path fullnode.yaml --download 500000 --execute 500000

cargo build --release --bin ping_service
time ./target/release/ping_service --my-id 1
```

# Servers

## Agents

An agent is a self-contained program that receives and sends messages to other agents. Services are implemented by an agent or a group of agents. E.g. A full-node service may comprise of Sequencing and Execution Agents.

The type of agents currently implemented in this repository includes:

Echo -- Prints every message it receives
Ping -- Periodically sends a ping to a target

To implement a new type of agent, one simply implements a struct that implements the `Agent` trait.

## Configuration files

A configuration defines a global system configuration. It specifies the mapping of agents to servers, and the attributes of each agent.
Configuration files are written in the JSON language, where each agent is indexed by a unique global identifier.

Each JSON agent object has the following fields:

kind -- A string describing the type of the agent, to be decoded by the driver.

ip and port -- The TCP listening address of the server. To create all-to-all TCP connections,each server will attempt to connect to all servers with a smaller global identifier.

attr -- Attributes are arguments to the Agent program.

![server architecture](server-architecture.png)
*Server architecture*