---
title: How Sui Differs from Other Blockchains
---

This is a high-level overview of the differences in approach between Sui and other blockchain systems. This document is intended for potential adopters of Sui so they may decide whether it fits your use cases. See How Sui Works for a summary of Sui’s processes and approaches.

Here are Sui's key features:

- High throughput and low latency (enables low cost with fixed hardware)

- Causal order vs total order (enables massively parallel execution)

- Move and object-centric data model (enables composable objects/NFTs)

- Asset-centric programming model

- Easier developer experience


## Authorities vs validators/miners

An authority plays a role similar to "validators" or "miners" in other blockchain systems. The key distinction between these roles (and the reason we insist on using a separate term) is that validators/miners are *active*, whereas authorities are *passive*. Broadly speaking:

* Miners/validators continuously participate in a global consensus protocol that requires multiple rounds of all-to-all communication between the participants. The goal is typically to agree on a *totally ordered* block of transactions and the result of their execution.

* Authorities do nothing until they receive a transaction or certificate from a user. Upon receiving a transaction or certificate, an authority need not communicate with other authorities in order to take action and advance its internal state machine. It may wish to communicate with other authorities to share certificates but need not do so.

## Causal order vs total order

Unlike most existing blockchain systems (and as the reader may have guessed from the description of write requests above), Sui does not impose a total order on the transactions submitted by clients. Instead, transactions are *causally* ordered--if a transaction `T1` produces output objects `O1` that are used as input objects in a transaction `T2`, an authority must execute `T1` before it executes `T2`. Note that `T2` need not use these objects directly for a causal relationship to exist--e.g., `T1` might produce output objects which are then used by `T3`, and `T2` might use `T3`'s output objects. However, transactions with no causal relationship can be processed by Sui authorities in any order.

## Writes

The problem is, there is a single increment for the entire blockchain world. This design mutualizes the ceremony of reaching consensus across required parties, which is effective yet costly and slow. Sui - a [proof-of-stake (PoS)](https://en.wikipedia.org/wiki/Proof_of_stake) blockchain - reduces this cost and latency by optimizing for the typical blockchain transaction of sending assets to another account.

Sui recognizes the only view needed to judge whether single-writer transactions are suitable is of that sender’s account. Sui does not need information from the rest of the world. Further, Sui supports more complex transactions with its object-centric focus and Move’s strong ownership model; these complex transitions can determine what part of the blockchain world must be seen to confirm transaction suitability and validity.

In this manner, Sui enables multi-lane processing and eliminates the head-of-line blocking. No longer must all other transactions in the world wait for the completion of the first transaction’s increment in a single lane. Sui provides a lane of the appropriate breadth for each transaction: simple sends require viewing only the sender account; more complex transactions may need to see more of the world’s state, and they will need to declare the required views explicitly.

Sui’s architecture allows you to do head-of-line blocking on only the sender. Each sender can send only one transaction at a time. And that transaction blocks no one else on the network from sending transactions. Sui assumes complex, interdependent transactions are the exception rather than the rule; most transactions are independent from one another. Sui and Move represent all of these transactions faithfully.

Because Sui limits the sender to one transaction at a time, it is imperative the transactions finish quickly. Sui offers these optimizations to speed transaction completion:

* Simpler algorithms are used to determine state, since less of the world’s state must be evaluated. These algorithms are based upon [Byzantine Consistent Broadcast](https://www.bc.edu/content/dam/bc1/schools/mcas/cs/pdf/honors-thesis/Thesis_Yifan-Zhang.pdf).
* Transaction sessions are interactive to ensure at-once processing and vote gathering. Instead of an asynchronous fire-and-forget model where transactions may take minutes or even hours, Sui transactions can finish in a subsecond.

A traditional blockchain operates via a single broadcasted request and awaits approval of the transaction, polling the validators for an answer sometime later. Either end users or the gateway must do a little more work and then get: low latency and better security. Simple broadcast transactions are completed instantly. Remember, no private keys are ever exchanged.

Each authority has weight according to standard Proof of Stake. Bonding their stake for the duration of the transaction. If all goes well, get a small fee. If proved cheating, you sacrifice your stake. For example, equivocation  - or inconsistent/duplicate voting - will cost you your stake.

## Reads

Now that you know how Sui handles writes, you should understand its management of reads from the blockchain. It’s all about the reads. Opinions have their limits.

Sui uses *causal order*, not total order. Every object in Sui has a version. Every certified (committed) transaction results in a new version. For example, an addition to an NFT would result in a new object. The transaction may have several objects as dependents. Objects come with its *family history*, a generational set of new versioned objects, since often the first object is mutated.

Since changes create new objects with a new version, Sui creates a narrow family tree starting from genesis. In Sui, as in life, you are most interested in your specific family, not the entire world’s genetic history. Sui relies upon no view of other family trees, only the one tied to the account making the transaction.

In a traditional blockchain, families are ordered against one another to calculate a *total order*. This then requires querying a massive blob for the precise information needed. Disk I/O becomes a blocker. Some blockchains now require SSDs on their validators as a result.

## Sui limitations

### Totality is hard to achieve

Sui can make total queries more difficult. And some total queries are valid. For example, you:

* want to join a network as a validator/authority and download total state to disk first.
* are a federal regulator auditing a blockchain for large transactions.
* must review your own exchange for account reconciliation since it is susceptible to crashes in underlying currency  that might result in money being created.

Sui solves this with checkpoints. A checkpoint is established every time an increment is added to a blockchain resulting from a certified transaction. Blocks work much like a [write ahead log](https://en.wikipedia.org/wiki/Write-ahead_logging) that stores state prior to full execution of a program. The calls in that program represent a *smart contract* in a blockchain. A block contains not only the transactions but also commitments to the state of the blockchain before and after the transactions.

Sui uses the state commitment that arrives upon *epoch*. Sui requires a single answer from the multiple authorities and leverages an accessory protocol to derive the hash representing the state of the blockchain. This protocol consumes little bandwidth and does not impede the ingestion of transactions. Authorities produce checkpoints at every epoch change. Sui requires the authorities to also produce checkpoints even more frequently. So you may use these checkpoints to audit the blockchain with some effort.

### Defining transactions that depend on shared state is difficult

Move’s strong ownership model ensures only the owner may change (mutate) the state of their objects (assets). They may transfer those objects to another user who may then modify those objects. By default, in Sui everything is owned by someone. You cannot touch someone else’s state. Only you can change state, such as transferring ownership of objects.

Where this can become problematic is in transactions where objects are shared, and you must reach consensus. To gain a total view of the world that is fresh in Sui, you must listen to [2f +1 authorities](https://www.gsd.inesc-id.pt/~mpc/pubs/bc2f+1.pdf), meaning you must query every single authority on the network. Sui is Byzantine Fault Tolerant to ensure that one third of the total stake can fail or even be controlled by a malicious party and the network will still succeed. Every authority has a stake in the blockchain, and the total amounts to 3f +1 authorities. Transactions requiring a total query need approval by >⅔ of *all* authorities.

Some blockchains use a severely unequal weight in stake for their validators. A small percentage of the validators have most of the stake in a blockchain of hundreds or thousands of validators. These blockchains can more readily be used to make votes on transactions look valid when they are not. Honest validators in such systems can be easily overwhelmed and even shut down, potentially resulting in even network failures.

For example, some blockchains use a heavily weighted set of validators that make it easy for a validator to make a transaction appear valid when not. This is prone to failures.

Sui’s causal order is less prone to failure, since it does not require a total read of the entire network’s state. With just 67% approval of the polled authorities, Sui processes the transaction.

Sui uses causal order. Shared objects are an explicit construct. You must wait for the resultive ordering of consensus (Byzantine agreement).

Only objects that share mutable state must be concerned with the total order of the transaction. These take more time.

At the end of the process, you receive a certificate indicating the transaction is processed. Optionally, you may also receive the balance of the account and other details, simply by waiting online. Here are the flows depending upon object ownership:

* Single-writer objects: Broadcast > Votes return > Certificate > Certificate artifact (optional)
* Multi-writer objects: Broadcast -> Votes -> Certificate -> Consensus -> Certificate -> Certificate artifact (optional)

You can gain complete transparency by collecting certificate details, ensuring the transaction is carried out precisely as intended (for instance, transactions in games are carried out by the rules).
