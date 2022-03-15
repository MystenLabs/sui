---
title: Understand Sui Security
---

This page provides an overview of the major guarantees Sui provides in terms of Security.

Sui asset owners
and smart contract designers can start learning here about the mechanisms available to secure their
assets, and the assurances Sui provides for them. Smart contract designers can also learn about the overall
Sui security architecture to ensure the asset types they design leverage Sui to provide a secure experience
to the asset holders. 

Before diving into this be sure to read [How Sui Works?](how-sui-works.md) to familiarize yourself with 
the basic components of Sui.

## Security features

We designed Sui to provide very high security guarantees to asset owners. We ensure that assets on Sui can be used
only by their owners, according to the logic pre-defined by smart contracts that can be audited, and that the network will be available 
to process them correctly despite some of the authorities operating Sui not following the protocol correctly (fault tolerance). 

The security features of the Sui system ensure a number of properties:

* Only the owner of an owned asset can authorize a transaction that operates on this asset. Authoritzation is performed through the use of a private signature key that is known only to the asset owner.
* Everyone can operate on shared assets or immutable assets, but additional access control logic can be implemented by the smart contract. 
* Transactions operate on assets according to predefined rules set by the smart contract creator that defined the asset type. These are expressed using the Move language.
* Once a transaction is finalized, its effects - namely changes to the assets it operates on or new assets created - will be persisted, and the resulting 
  assets will be available for further processing.
* The Sui system operates through a protocol between a set of independent authorities. Yet all its security properties are preserved 
  when a small subset of the authorities do not follow the protocol.
* All operations in Sui can be audited to ensure any assets have been correctly processed. This implies all operations on Sui
  are visible to all, and users may wish to use multiple different addresses to protect their privacy.
* Authorities are determined periodically through users of Sui locking and delegating SUI tokens to one or more authorities.

## Security architecture

The Sui system is operated by a set of authorities that process transactions. They implement the Sui protocol that allows them to 
reach agreement on valid transactions submitted and processed in the system. 

The agreement protocols Sui uses tolerate a fraction of authorities not following the Sui protocol correctly, through 
the use of Byzantine fault tolerant broadcast and consensus. Specifically, each authority has some voting power,
assigned to it through the process of users delegating / voting for them using their SUI tokens. Sui maintains
all its security properties if over 2/3 of the stake is assigned to authorities that follow the protocol. However,
a number of auditing properties are maintained even if more authorities are faulty.

### Addresses and ownership

A Sui transaction is valid and can proceed only if the owner of all owned assets it operates on digitally signs it with their private 
signature key (currently using the EdDSA algorithm). This signature key can be kept private by the user and not be shared with 
anyone else. As a result, it is not feasible for any other party to operate on an owned asset of a user undetected, even if all authorities 
do not follow the protocol.

A private signature key also corresponds to a public address on the Sui network that can be used to send a user assets or
allow smart contracts to define custom access control logic. A user may have one or more addresses corresponding to 
multiple signature keys for convenience or privacy reasons. An address does not need any pre-registration, and sending
an asset to an address automatically creates this address on the network. However, this means that users should
be careful to check the recipient address of transfers, or parties involved in any other operations, as sending assets to
an in correct address may have irrevocable effects.

### Smart contracts define asset types and their logic

All assets have a type that is defined within a Sui Smart Contract. Sui provides a few system contracts, such as these used to 
manage the SUI native token, yet also allows anyone to write and submit custom smart contracts. A transaction on an asset type 
can call operations defined in only the smart contract that defined the asset type, and is constrained by the logic in the contract. 

For this reason, users are encouraged to operate on their assets using smart contracts they trust, that they or others 
they trust have audited, and understand the logic they define for operations on their assets. Sui smart contracts are 
defined as immutable assets to allow third parties to audit them and also prevent their modification to increase assurance. 
The Move smart contract language that Sui uses is designed with ease of audit and verification in mind. You may be 
interested in our introduction to [Smart Contracts in Move](../build/move.md).

Shared assets allow multiple users to operate on them through transactions; that may include some of their owned assets
as well as one or more shared assets. These shared assets represent data and logic used to implement protocols that mediate
between different users in a safe way, according to the smart contract that defined the type of the shared asset. Sui allows
all users to create transactions involving shared assets. But the smart contract type may define additional restrictions
on which address and how the shared assets may be used.

### Transaction finality

A valid transaction submitted to all authorities to be certified and its certificate also has to be submitted to all authorities
to be finalized. Even if a subset of authorities do not follow the protocol, the transaction can be finalized through the
remaining authorities that correctly follow the Sui protocol. This is achieved throught the use of cryptographic 
Byzantine fault tolerant agremment protocols for broadcast and consensus defined by the Sui protocol. These protocols
ensure both safety, meaning that the incorrect authorities cannot convince correct clients of incorrect state, and 
liveness, meaning that incorrect authorities cannot prevent transaction processing.

All transactions in Sui have to be associated with a gas asset to cover the cost of processing by Sui. A valid 
transaction may result in a status of successful execution or an aborted execution. An execution may abort due to a 
condition within the smart contract defining the asset, or because it has ran out of sufficient gas to pay for
the cost of execution. In cases of success, the effects of the operation will be finalized; otherwise, the state of 
assets in the transaction is not changed. However, the gas asset is always charged some amount of gas, to aleviate
denial-of-service attacks on the system as a whole.

A user client can perform the process of submitting the transaction and certificate itself or rely on third party 
services to submit the transaction and interact with authorities. Such third parties need not have user private signature keys and cannot forge transactions on the users' behalf.
They can reassure a user client a transaction has been finalized through a set of signatures from 
authorities attesting to the transactions finality and its effects. After that point, the users can be assured that 
changes the transaction resulted in are final. 

### Auditing and privacy

Sui authorities provide facilities for users to read all asserts they store, as well as the historical record of
transactions they have processed that led to these assets. Authorities also provide cryptographic evidence of the full
chain of transactions that contributed to an asset state. User clients can request and validate this chain of 
evidence to ensure all operations were correct and the result of the collective agreement between authorities. 
Services that operate full replicas, mirroring the state of one or more authorities, perform such audits routinely.

The extreme public auditability of Sui also implies that all transactions and assets within Sui are publicly
visible. Users that are mindful of their privacy may use multiple addresses to benefit from some degree of
pseudonymity, or third-party custodial or non-custodial services. Specific smart contracts with additional cryptographic privacy protections can also be provided by third parties.

### Censorship-resistance and openness

Sui uses the established Delegated Proof-of Stake model to periodically determine its set of authorities. Users can lock and delegate their SUI tokens in each epoch to determine the committee of authorities that operate the Sui network in the next epoch. Anyone with over a minimum 
amount of delegated stake may operate an authority. 

Authorities operate the network and provide
rewards to users that delegated their Sui to support them as validators, through gas fee income. Authorities with poor reliability, and in turn the users that delegated their stake to them, may receive a lower reward. But user stake cannot be confiscated away either by malicious authorities or anyone in the network.

This mechanism ensures that authorities are accountable to Sui users and can be rotated out at the first sign 
of unreliability or misbehavior, including noticed attempts to censor valid transactions. Through choices of authorities, and the protocol 
they are willing to operate, Sui users also have a meaningful say on the future evolution of the Sui system.

## Further reading

If you are looking for an in-depth technical explanation of the computer science behind Sui security, you 
may have a look at our white paper on [The Sui Smart Contracts Platform](../../paper/sui.pdf).
