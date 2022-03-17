---
title: How Sui Differs from Other Blockchains
---

This is a high-level overview of the differences in approach between Sui and other blockchain systems. This document is intended for potential adopters of Sui so they may decide whether it fits your use cases. See How Sui Works for a summary of Sui’s processes and approaches.

Here are Sui's key features:

* Causal order vs total order (enables massively parallel execution)

* [Sui's variant of Move](../build/move.md) and its object-centric data model (enables composable objects/NFTs)

* Easier developer experience with the blockchain-oriented [Move programming language](https://github.com/MystenLabs/awesome-move)


## Authorities vs validators/miners

An authority plays a role similar to "validators" or "miners" in other blockchain systems. The key distinction between these roles (and the reason we insist on using a separate term) is that validators/miners are *active*, whereas authorities are *passive* for the main type of Sui transaction involving single-writer objects. Broadly speaking, to deal with a transfer:

* Miners/validators continuously participate in a global consensus protocol that requires multiple rounds of all-to-all communication between the participants. The goal is typically to agree on a *totally ordered* block of transactions and the result of their execution.

* Authorities do nothing until they receive a transaction or certificate from a user. Upon receiving a transaction or certificate, an authority need not communicate with other authorities in order to take action and advance its internal state machine. It may wish to communicate with other authorities to share certificates but need not do so.

## Causal order vs total order

Unlike most existing blockchain systems (and as the reader may have guessed from the description of write requests above), Sui does not always impose a total order on the transactions submitted by clients, with shared objects being the exception. Instead, most transactions are *causally* ordered--if a transaction `T1` produces output objects `O1` that are used as input objects in a transaction `T2`, an authority must execute `T1` before it executes `T2`. Note that `T2` need not use these objects directly for a causal relationship to exist--e.g., `T1` might produce output objects which are then used by `T3`, and `T2` might use `T3`'s output objects. However, transactions with no causal relationship can be processed by Sui authorities in any order.

## Writes

In a traditional blockchain, the problem is that there is a single increment for the entire blockchain's world. This design mutualizes the ceremony of reaching consensus across required parties, which is effective yet slow. Sui - a [proof-of-stake (PoS)](https://en.wikipedia.org/wiki/Proof_of_stake) blockchain - reduces this cost and latency by optimizing for the typical transaction sending assets to another account.

Sui recognizes the only view needed to judge whether single-writer transactions are suitable is of that sender’s account. Sui does not need information from the rest of the world. Further, Sui supports more complex transactions with its object-centric focus and Move’s strong ownership model; these complex transitions can determine what part of the blockchain world must be seen to confirm transaction suitability and validity.

In this manner, Sui enables multi-lane processing and eliminates [head-of-line blocking](https://en.wikipedia.org/wiki/Head-of-line_blocking). No longer must all other transactions in the world wait for the completion of the first transaction’s increment in a single lane. Sui provides a lane of the appropriate breadth for each transaction: simple sends require viewing only the sender account; more complex transactions may need to see more of the world’s state - but not all of it, and they will need to declare the required views explicitly.

Sui’s architecture minimizes the impact of checking the validity of a transaction: each sender can send only one, non-equivocating transaction at a time. And that transaction blocks no one else on the network from sending transactions. Sui assumes complex, interdependent transactions are the exception rather than the rule; most transactions are independent from one another, merely making payments online. Sui and Move represent all of these transactions faithfully.

Because Sui limits the sender to one transaction at a time, it is imperative the transactions finalize quickly. Sui offers these optimizations to speed transaction completion:

* For transactions dependent on a single writer, Sui uses a lighter communication algorithm based on
  [Byzantine Consistent Broadcast](https://link.springer.com/book/10.1007/978-3-642-15260-3).
* Transaction sessions are interactive to ensure at-once processing and vote gathering. Instead of a fire-and-forget model where transactions may take minutes or even hours, Sui transactions can finish in under a second.

A traditional blockchain client operates via a single send request and awaits approval of the transaction, polling the validators for an answer sometime later. Either end users or the gateway must do a little more work and then get: low latency and better security. Simple broadcast transactions are completed immediately. Remember, no private keys are ever revealed.

## Reads

Now that you know how Sui handles writes, you should remarks its management of reads follows the same object model.

If you are interested in a specific set of objects and their history, Sui reads are authenticated at a high granularity and served with a low average latency. If you instead need a * totality* property to, for example, conduct continuous whole-chain audits, Sui offers periodic checkpoints that support this use case.

Sui uses *causal order*, not total order. Every object in Sui has a version, and every valid transaction results in new versions for the objects it touches. For example, an addition to an NFT would result in a new object. The transaction may have several objects as dependents. Objects come with its *family history*, a generational set of new versioned objects.

Since changes create new objects with a new version, Sui creates a narrow family tree starting from genesis. In Sui, as in life, you are most interested in your specific family, not the entire world’s genetic history. Sui relies upon no view of other family trees, only the one tied to the account making the transaction.

By contrast, in a traditional blockchain, all families are ordered against one another to calculate a *total order*. This then requires querying a massive blob for the precise information needed, and disk I/O becomes a blocker. Some blockchains now require SSDs on their validators as a result.

## Sui's limitations

### Totality is harder to achieve using just Sui's default mode

Sui's default model can make reads of the whole blockchains a bit harder to serve. Such exhaustive reads, though rare, are perfectly legitimate. They may include:

* wanting to join a network as a new authority
* wanting to audit the whole chain
* exposing the whole chain to downstream customers

Sui solves this with the state checkpoints resulting in state commitments. Sui will produce those checkpoints on every epoch change, and at regular intervals as long as they do not impede the ingestion of transactions.

The checkpoints carry cryptographic signatures that guarantee they form a consensual snapshot of the state of the Sui blockchain. We discuss how it is produced in the next section.

### Defining transactions that depend on shared state requires ordering

Move’s strong ownership model ensures only the owner may change (mutate) the state of their objects (assets). They may transfer those objects to another user who may then modify those objects. By default, in Sui everything is owned by someone. You cannot touch someone else’s state. Only you can change state, such as transferring ownership of objects.

Where this can become problematic is in transactions where objects are mutable by two writers. This may include the following use cases:
- a time-bound auction, where several bidders must enter their bid before a deadline
- an open-order, where several traders may fulfill the same proposed trade

In this case, ordering transactions with respect to each other is vital to lead to a valid resolution, but no actor's action depends on the other. The way Sui resolves this is to resort to a consensus mechanism. While Sui's chosen consensus mechanism will be efficient and high-throughput (as in, e.g. [Narwhal & Tusk](https://arxiv.org/abs/2105.11827)), it still obeys the asymptotics and limitations of any consensus algorithm : polynomial worst-case complexity, requiring active inter-authority messages, etc.
