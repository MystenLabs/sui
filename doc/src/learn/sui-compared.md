---
title: Sui compared to another blockchain & Sui limitations
---

# Sui compared to other Blockchains

This is a high-level overview of the differences in approach between Sui and other blockchain systems.

## Block chains

Blockchains validators collectively build a shared accumulator: a representation of the state of the blockchain, a chain to which they add increments over time, called blocks. In blockchains that offer finality, every time validators want to make an incremental addition to the blockchain, i.e. a block proposal, they enter into a consensus protocol. This protocol lets them form an agreement over the current state of the chain, whether the proposed increment is suitable and valid, and what the state of the chain will be after the new addition. 

This method of maintaining common state over time has known practical success over the last 14 years or so, using a wealth of theory from the last 50 years of research in the field of Byzantine Fault Tolerant distributed systems. 

Yet it is inherently sequential: increments to the chain are added one at a time, like pearls on a string. In practice, this approach pauses the influx of transactions (often stored in a "mempool"), while the current block is under consideration.

## Sui's approach to validating new transactions

A lot of transactions do not have complex interdependencies with other, arbitrary parts of the state of the blockchain. Often financial users just want to send an asset to a recipient, and the only data required to gauge whether this simple transaction is admissible is a fresh view of the sender's account. Hence Sui takes the approach of only "stopping the world" for the relevant piece of data rather than the whole chain -- in this case, the account of the sender, which can only send one transaction at a time.

Sui further expands this approach to more involved transactions that may explicilty depend on multiple elements under their sender's control, using an [object model][Objects] and leveraging [Move][Move]'s strong ownership model. By requiring that dependencies be explicit, Sui applies a "multi-lane" approach to transaction validation, making sure those independent transaction flows can progress without impediment from the others.

This doesn't mean that Sui is a platform never orders transactions with respect to each other, or that it we allows owners to only affect their owned microcosm of objects. Sui will also process transactions that have an effect on some shared state, in a rigorous, consensus-ordered manner. They're just not the default use case.
## A collaborative approach to transaction submission

Sui validates transactions individually, rather than batching them in the traditional blocks. An advantage of this approach is that each successful transaction quickly obtains a certificate of finality which proves to anyone that the transaction will be processed by the Sui network, but the process of submitting a transaction is a bit more involved. Whereas an usual blockchain can accept a bunch of transactions from the same author in a "fire and forget" mode, Sui transaction submission follows the following steps:
1. the sender broadcasts a transaction to all Sui authorities,
2. the Sui authorities send individual votes on this transaction to the sender, 
3. the sender collects a Byzantine-resistant-majority of these votes into a Certificate, and broadcasts it to all Sui authorities,
4. (optional) the sender collects a certificate detailing the effects of the transaction.

While those steps demand more of the sender, performing them efficiently can still yield a cryptographic proof of finality well under a second. Aside from crafting the original transaction itself, the session management for a transaction does not require access to any private keys, and can be delegated to a third party. Since we live in a world where the cost of bandwith is diminishing steadily, we hope to see an ecosystem of services that will find it easy, fun, and perhaps even profitable to manage transaction submission on behalf of users. We provide a reference implementation of such a service, called the Sui Gateway service.

In exchange for this collaboration, users of Sui get a much lower latency: their transaction is certified and final as soon as three sequential messaging & processing steps have occured between them and authorities (one and a half round-trips), something that can be routinely achieved under a second. Indeed, Sui's object model unlocks parallel execution, and means that authorities can be scaled. This means that processing a transaction in a well-resourced Sui node is only limited by those network transmission delays, rather than being bound by storage IO and compute power, a more common blocker in classical blockchain architectures.

## A different approach to state

Because Sui focuses on managing specific objects rather than a single aggregate of state, they also report on them in this way:
- Every object in Sui has a unique version, and 
- every new version is created from a transaction which may involve several dependencies, themselves versioned objects. 

As a consequence, a Sui authority -- or any other node with a copy of the state -- can exhibit a causal history of an object, showing its history since genesis. Sui explicitly makes the bet that in most cases, the ordering of that causal history with the causal history of another oject is irrelevant, and in the few cases where it is, makes this relationship explicit in the data. 


# Sui Limitations

TODO: exhaustive reads, shared mutable state pegged on ordering.
