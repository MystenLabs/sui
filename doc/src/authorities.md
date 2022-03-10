# Authorities

The Sui network is operated by a set of independent *authorities*, each running its own instance of the Sui software on a separate machine (or a [sharded](TODO) cluster of machines operated by the same entity). An authority participates in the network by handling [read](TODO) and write requests sent by [clients](TODO). This section focuses on the latter.

TODO: Populate and link to or remove the placeholder TODOs and references above and below.

Sui uses proof of stake (PoS) to determine which authorities operate the network and their voting power. Authorities are incentivized to participate in good faith via a share of transaction fees, staking rewards, and slashing to punish misbehavior. See [Governance, Incentives, and Rewards](TODO) for more detail.

## Epochs
Operation of the Sui network is temporally partitioned into non-overlapping, fixed-duration (e.g. 24-hour) *epochs*. During a particular epoch, the set of authorities participating in the network is fixed. At an epoch boundary, [reconfiguration](TODO) occurs and can change the set of authorities participating in the network and their voting power. Conceptually, reconfiguration starts a new instance of the Sui protocol with the previous epoch's final state as [genesis](objects.md#the-transaction-object-dag-relating-objects-and-transactions) and the new set of authorities as the operators.

EDITORIAL NOTE: not sure if the fixed duration part matches our plans--let me know

## Committees
A *committee* is a set of authorities whose combined voting power is >2/3 of the total during a particular epoch. For example, in a Sui instance operated by four authorities that all have the same voting power, any group containing three authorities is a committee.

TODO: When would they not have the same voting power?

The committee size of >2/3 is chosen to ensure *[Byzantine fault](https://en.wikipedia.org/wiki/Byzantine_fault) tolerance (BFT)*. As we will see, an authority will  commit a transaction (i.e., durably store the transaction and update its internal state with the effects of the transaction) only if it is accompanied by cryptographic signatures from a committee. We call the combination of the transaction and the committee signatures on its bytes a *certificate*. The policy of  committing only certificates ensures Byzantine fault tolerance: if <2/3 of the authorities faithfully follow the protocol, they are guaranteed to eventually agree on both the set of committed certificates and their effects.

EDITORIAL NOTE: someone with BFT chops please correct/expand on this. Also confirm the <2/3 reference above is correct.

## Write requests
An authority can handle two types of write requests: transactions and certificates. At a high level, a client:
* communicates a transaction to a quorum of authorities to collect the signatures required to form a certificate.
* submits a certificate to an authority to commit state changes on that authority.

EDITORIAL NOTE: A diagram would be useful here.

### Transactions
When an authority receives a transaction from a client, it will first perform [transaction validity checks](TODO) (e.g., validity of the sender's signature). If the checks pass, the authority will sign the transaction bytes and return the signature to the client. The client repeats this process with multiple authorities until it has collected signatures on its transaction from a committee, thereby forming a certificate.

Note that the process of collecting authority signatures on a transaction into a certificate and the process of submitting certifcates can be performed in parallel. The client can simultaneously broadcast transactions/certificates to an arbitrary number of authorities. Alternatively, a client can outsource either or both of these tasks to a third-party service provider. This provider must be trusted for liveness (e.g., it can refuse to form a certificate), but not for safety (e.g., it cannot change the effects of the transaction).

TODO: Clarify the final clause with an explanation of why the provider must *not* be trusted for safety.

### Certificates
Once the client forms a certificate, it submits the certificate to an authority, which will perform [certificate validity checks](TODO) (e.g., ensuring the signers are authorities in the current epoch, and the signatures are cryptographically valid). If the checks pass, the auhority will execute the transaction inside the certificate. Execution of a transaction will either succeed and commit all of its effects to the ledger, or [abort](TODO) (e.g., due to an explicit `abort` instruction, a runtime error such as divison by zero, or exceeding the maximum gas budget) and have no effects other than debiting the transaction's gas input. In either case, the transaction will durably store the certificate indexed by the hash of its inner transaction.

TODO: Link to gas definition on first reference.

As with transactions, we note that the process of sharing a certificate with authorities can be parallelized and (if desired) outsourced to a third-party service provider. A client should broadcast its certificate to >1/3 of the authorities to ensure that (up to BFT assumptions) at least one honest authority has executed and committed the certificate. Other authorities may learn about the certificate via [inter-authority state sync](TODO) or via [client-assisted state sync](TODO).

EDITORIAL NOTE: please check that my advice about how many authorities a client should broadcast certs to looks good.

## Further reading

* Transactions take objects as input and produced objects as output--check out the [objects](objects.md) section to learn more about the structure and attributes of objects.
* Sui supports several different transaction types--see the [transactions](transactions.md) section for full details.
