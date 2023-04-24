---
title: Validators
---

The Sui network is operated by a set of independent *validators*, each running its own instance of the Sui software on a separate machine (or a sharded cluster of machines operated by the same entity). A validator participates in the network by handling read and write requests sent by clients. This section focuses on the latter.

To learn how to set up and run a Sui Validator node, including how staking and rewards work, see [Sui Validator Node](../build/validator-node.md).

Sui uses Delegated Proof-of-Stake (DPoS) to determine which validators operate the network and their voting power. Validators are incentivized to participate in good faith via a share of transaction fees, staking rewards, and slashing stake and staking rewards in case of misbehavior.

## Epochs

Operation of the Sui network is temporally partitioned into non-overlapping, approximate fixed-duration (e.g. 24-hour) *epochs*. During a particular epoch, the set of validators participating in the network and their voting power is fixed. At an epoch boundary, reconfiguration may occur and can change the set of validators participating in the network and their voting power. Conceptually, reconfiguration starts a new instance of the Sui protocol with the previous epoch's final state as genesis and the new set of validators as the operators. Besides validator set changes, tokenomics operations such as staking/un-staking, and distribution of staking rewards are also processed at epoch boundaries.

## Quorums

A *quorum* is a set of validators whose combined voting power is >2/3 of the total during a particular epoch. For example, in a Sui instance operated by four validators that all have the same voting power, any group containing three validators is a quorum.

The quorum size of >2/3 is chosen to ensure *[Byzantine fault](https://en.wikipedia.org/wiki/Byzantine_fault) tolerance (BFT)*. A validator will commit a transaction (i.e., durably store the transaction and update its internal state with the effects of the transaction) only if it is accompanied by cryptographic signatures from a quorum. Sui calls the combination of the transaction and the quorum signatures on its bytes a *certificate*. The policy of committing only certificates ensures Byzantine fault tolerance: if >2/3 of the validators faithfully follow the protocol, they are guaranteed to eventually agree on both the set of committed certificates and their effects.

## Write requests

A validator can handle two types of write requests: transactions and certificates. At a high level, a client:

* communicates a transaction to a quorum of validators to collect the signatures required to form a certificate.
* submits a certificate to a validator to commit state changes on that validator.

### Transactions

When a validator receives a transaction from a client, it will first perform transaction validity checks (e.g., validity of the sender's signature). If the checks pass, the validator locks all owned-objects and signs the transaction bytes. It then returns the signature to the client. The client repeats this process with multiple validators until it has collected signatures on its transaction from a quorum, thereby forming a certificate.

Note that the process of collecting validator signatures on a transaction into a certificate and the process of submitting certificates can be performed in parallel. The client can simultaneously multicast transactions/certificates to an arbitrary number of validators. Alternatively, a client can outsource either or both of these tasks to a third-party service provider. This provider must be trusted for liveness (e.g., it can refuse to form a certificate), but not for safety (e.g., it cannot change the effects of the transaction, and does not need the user's secret key).

### Certificates

Once the client forms a certificate, it submits it to the validators, which will perform certificate validity checks (e.g., ensuring the signers are validators in the current epoch, and the signatures are cryptographically valid). If the checks pass, the validators will execute the transaction inside the certificate. Execution of a transaction will either succeed and commit all of its effects, or abort (e.g., due to an explicit `abort` instruction, a runtime error such as division by zero, or exceeding the maximum gas budget) and have no effects other than debiting the transaction's gas input. In either case, the validator will durably store the certificate indexed by the hash of its inner transaction. 

If a client collects a quorum of signatures on the effects of the transaction then the client has a promise of finality. This means that this effects will persist on the shared database and actually be committed and visible to everyone by the end of the epoch. This does not mean that the latency is a full epoch, since the effects certificate can be used to convince anyone of the transactions finality as well as to access the effects and issue new transactions.
As with transactions, note that the process of sharing a certificate with validators can be parallelized and (if desired) outsourced to a third-party service provider. 

## The role of Narwhal and Bullshark

Sui takes advantage of [Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus](consensus.md) and the Tusk successor [Bullshark](https://arxiv.org/abs/2201.05677). Narwhal/Bullshark (N/B) are also being implemented in Sui so that when Byzantine agreement is required it uses a high-throughput DAG-based consensus to manage shared locks while execution on different shared objects is parallelized.

Narwhal enables the parallel ordering of transactions into batches that are collected into concurrently proposed blocks, and Bullshark defines an algorithm for executing the DAG that these blocks form. N/B combined builds a DAG of blocks, concurrently proposed, and creates an order between those blocks as a byproduct of the building of the DAG. But that order is overlaid on top of the causal order of Sui transactions (the "payload" of Narwhal/Bullshark here), and does not substitute for it:

* Narwhal/Bullshark operates in OX, rather than XO mode (O = order, X = execute); the execution occurs after the Narwhal/Bullshark ordering.
* The output of N/B is therefore a sequence of transactions, with interdependencies stored in the transaction data itself.

Consensus sequences certificates of transactions. These represent transactions that have already been presented to 2/3 of validators that checked that all their owned objects are available to be operated on and signed the transaction. Upon a certificate being sequenced, Sui sets the *lock* of the shared objects at the next available version to map to the execution of that certificate. So for example if you have a shared object X at version 2, and you sequence certificate T, Sui stores T -> [(X, 2)]. That is all you do when Sui reaches consensus, and as a result Sui can ingest a lot of sequenced transactions.

Now, once this is done Sui can execute all certificates that have their locks set, on one or multiple cores. Obviously, transactions for earlier versions of objects need to be processed first (causally), and that reduces the degree of concurrency. The read and write set of the transaction can be statically determined from its versioned object inputs--execution can only read/write an object that was an input to the transaction, or that was created by the transaction.

## Further reading

* Transactions take objects as input and produce objects as output&mdash;check out the [objects](../../learn/objects.md) section to learn more about the structure and attributes of objects.
* Sui supports several different transaction types&mdash;see the [transactions](../../learn/transactions.md) section for full details.
