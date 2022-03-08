## What makes Sui different?

TODO: WIP

Here are Sui's key features:

- High throughput and low latency (enables low cost with fixed hardware)
- Causal order vs total order (enables massively parallel execution)
- Move and object-centric data model (enables composable objects/NFTs)

### Authorities vs Validators/Miners
An authority plays a role similar to "validators" or "miners" in other blockchain systems. The key distinction between these roles (and the reason we insist on using a separate term) is that validators/miners are *active*, whereas authorities are *passive*. Broadly speaking:
* Miners/validators continuously participate in a global consensus protocol that requires multiple rounds of all-to-all communication between the participants. The goal is typically to agree on a *totally ordered* block of transactions and the result of their execution.
* Authorities do nothing until they receive a transaction or certificate from a user. Upon receiving a transaction or certificate, an authority need not communicate with other authorities in order to take action and advance its internal state machine. It may wish to communicate with other authorities to share certificates but need not do so.

## Causal order vs Total order
Unlike most existing blockchain systems (and as the reader may have guessed from the description of write requests above), Sui does not impose a total order on the transactions submitted by clients. Instead, transactions are *causally* ordered--if a transaction `T1` produces output objects `O1` that are used as input objects in a transaction `T2`, an authority must execute `T1` before it executes `T2`. Note that `T2` need not use these objects directly for a causal relationship to exist--e.g., `T1` might produce output objects which are then used by `T3`, and `T2` might use `T3`'s output objects. However, transactions with no causal relationship can be processed by Sui authorities in any order.
