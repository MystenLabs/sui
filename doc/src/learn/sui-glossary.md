---
title: Sui Glossary
---

Find terms used in Sui defined below. Where possible, we link to a canonical definition and focus upon Sui’s use of the term.


### Accumulator

An *accumulator* makes sure the transaction is received by a quorum of validators, collects a quorum of votes, submits the certificate to the validators, and replies to the client. The accumulator enables transactions to be certified. Sui offers a Gateway service that can assume the role of accumulator and collect votes on transactions from validators in Sui, saving end-users bandwidth.


### Causal order

[Causal order](https://www.scattered-thoughts.net/writing/causal-ordering/) is a representation of the relationship between transactions
and the objects they produce, laid out as dependencies. Validators cannot execute a transaction dependent on objects created by a prior
transaction that has not finished. Rather than total order, Sui uses causal order (a partial order).

For more information, see [Causal order vs total order](sui-compared.md#causal-order-vs-total-order). 


### Certificate

A certificate is the mechanism proving a transaction has been approved, or certified. Validators vote on transactions, and an aggregator collects
a Byzantine-resistant-majority of these votes into a certificate and broadcasts it to all Sui validators, thereby ensuring finality.


### Equivocation

Equivocation in blockchains is the malicious action of dishonest actors giving conflicting information for the same message, such as inconsistent or duplicate voting.


### Epoch

Operation of the Sui network is temporally partitioned into non-overlapping, fixed-duration *epochs*. During a particular epoch, the set of validators participating in the network is fixed.

For more information, see [Epochs](architecture/validators.md#epochs).


### Eventual consistency

[Eventual consistency](https://en.wikipedia.org/wiki/Eventual_consistency) is the consensus model employed by Sui; if one honest validator
certifies the transaction, all of the other honest validators will too eventually.


### Causal history

Causal history is the relationship between an object in Sui and its direct predecessors and successors. This history is essential to the causal
order Sui uses to process transactions. In contrast, other blockchains read the entire state of their world for each transaction,
introducing latency.


### Finality

[Finality](https://medium.com/mechanism-labs/finality-in-blockchain-consensus-d1f83c120a9a) is the assurance a transaction will not be revoked. This
stage is considered closure for an exchange or other blockchain transaction.


### Gas

[Gas](https://ethereum.org/en/developers/docs/gas/) refers to the computational effort required for executing operations on the Sui network. In Sui, gas is paid with the network's native currency SUI. The cost of executing a transaction in SUI units is referred to as the transaction fee.


### Genesis

Genesis is the initial act of creating accounts and gas objects. Sui provides a `genesis` command that allows users to create and inspect the genesis object setting up the network for operation.

For more information, see [Genesis](../build/cli-client.md#genesis).


### Gateway service

Sui provides a Gateway service that enables third parties, say app/game developers, to route transactions on behalf of users. Because Sui never requires
exchange of private keys, users can offload the bandwidth use of transaction submission (e.g. when operating from a mobile device) to an untrusted server.


### Multi-writer objects

Multi-writer objects are those owned by more than one account. Transactions affecting multi-writer objects require consensus in Sui. This contrasts with
those affecting only single-writer objects, which require only a confirmation of the owner’s account contents.

### Object

The basic unit of storage in Sui is object. In contrast to many other blockchains where storage is centered around accounts and each account contains a key-value store, Sui's storage is centered around objects. Sui objects come in these primary states:

* *Immutable* - the object cannot be modified.
* *Mutable* - the object can be changed.

Further, mutable objects are divided into these categories:

* *Owned* - the object can be modified only by its owner.
* *Shared* - the object can be modified by anyone.

Immutable objects do not need this distinction because they have no owner.

For more information, see [Sui Objects](../build/objects.md).

### Proof-of-stake

[Proof-of-stake](https://en.wikipedia.org/wiki/Proof_of_stake) is a blockchain consensus mechanism where the voting weights of validators or validators is proportional to a bonded amount of the network's native currency (called their stake in the network). This mitigates [Sybil attacks](https://en.wikipedia.org/wiki/Sybil_attack) by forcing bad actors to gain a large stake in the blockchain first.


### Smart contract

A [smart contract](https://en.wikipedia.org/wiki/Smart_contract) is an agreement based upon the protocol for conducting transactions in a blockchain. In Sui, smart contracts are written in the [Move](https://github.com/MystenLabs/awesome-move) programming language.


### Single-writer objects

Single-writer objects are owned by one account. In Sui, transactions affecting only single-writer objects owned by the same account may proceed with only a check of the sender’s account, greatly speeding transaction times.

### Sui/SUI

Sui refers to the Sui blockchain, the SUI currency, and the [Sui open source project](https://github.com/MystenLabs/sui/) as a whole.


### Total order

[Total order](https://en.wikipedia.org/wiki/Total_order) refers to the ordered presentation of the history of all transactions processed by a traditional blockchain up to a given time. This is maintained by many blockchain systems, as the only way to process transactions. In contrast, Sui uses a causal (partial) order wherever possible and safe.

For more information, see [Causal order vs total order](sui-compared#causal-order-vs-total-order). 


### Transfer

A transfer is switching the owner address of a token to a new one via command in Sui. This is accomplished via the
[Sui CLI client](../build/cli-client.md) command line interface. It is one of the more common of many commands
available in the CLI client.

For more information, see [Transferring objects](../build/cli-client.md#transferring-objects).


### Validator

A validator in Sui plays a passive role analogous to the more active role of validators and minors in other blockchains. In Sui,
validators do not continuously participate in the consensus protocol but are called into action only when receiving a transaction or
certificate.

For more information, see [Validators](architecture/validators.md).

