# Transactions

All updates to the Sui ledger happen via a transaction. This section describes the transaction types supported by Sui and explains how their execution changes the ledger.

TODO: Define *Gas* and settle upon spelling and capitalization right here.

## Transaction metadata

All Sui transactions have the following common metadata:
* Sender address: The address of the user sending this transaction.
* Gas Input: An object reference pointing to the object that will be used to pay for this transaction's execution and storage. This object must be owned by the user and must be of type `Sui::Coin::Coin<GAS>` (i.e., the Sui native currency).
* Gas Price: An unsigned integer specifying the number of native tokens per gas unit this transaction will pay. The gas price must always be nonzero.
* Maximum Gas Budget: The maximum number of gas units that can be expended by executing this transaction. If this budget is exceeded, transaction execution will [abort](TODO) and have no effects other than debiting the gas input. The gas input object must have a value higher than the gas price multiplied by the max gas, and this product is the maximum amount that the gas input object will be debited for the transaction.
* Epoch: The Sui epoch this transaction is intended for--see [Epochs](TODO).
* Type: A call, publish, or native transaction and its type-specific-data (see below).
* Authenticator: A cryptographic signature on the [Binary Canonical Serialization (BCS)](https://docs.rs/bcs/latest/bcs/)-encoded bytes of the data above, and a public key that both verifies against the signature and is cryptographically committed to by the sender address--(see [Addresses and Authenticators](TODO) for more details).

EDITORIAL NOTE: things are organized slightly differently today. Gas input and max gas live in types, but I think they should be moved up here since all transactions need them. Gas price does not exist yet, but eventually should. Epoch does not exist yet, but eventually should. Authenticator does not yet exist in the current form, but will eventually.

## Move call transaction

This transaction type is a *smart contract call* that invokes a function in a published Move package with objects owned by the sender and pure values (e.g., integers) as inputs. Executing a function may read, write, mutate, and transfer these input objects, as well as other objects created during execution.

In addition to the common metadata above, a call transaction includes the following fields:
* Package: An object reference pointing to a previously published Move package object.
* Module: A UTF-8 string specifying the name of a Move module in the package.
* Function: A UTF-8 string specifying the name of a function inside the module. The function must be a valid [entrypoint](TODO).
* Type Inputs: A list of Move types that will be bound to the type parameters of the function.
* Object Inputs: A list of unique object references pointing to objects that will be passed to this function. Each object must either be owned by the sender or immutable. *The gas input object from above cannot also appear as an object input.*
* Pure Inputs: A list of BCS-encoded values that will be bound to the parameters of the function. Pure inputs must be primitive types (i.e. addresses, object ID's, strings, bytes, integers, or booleans)--they cannot be objects.

## Move publish transaction

This transaction type publishes a new Move package as an immutable object. Once the package has been published, its public functions and types can be used by future packages, and its entrypoint functions can be called by future transactions.

In addition to the common metadata above, a publish transaction includes the following:
* Package Bytes: A list of Move bytecode modules topologically sorted by their dependency relationship (i.e., leaves in the dependency graph must appear earlier in the list). These modules will be deserialized, [verified](TODO), and [linked](TODO) against their dependencies. In addition, each module's [initializer function](TODO) will be invoked in the order specified by the list.

TODO: Either add to the list of one above or combine with the intro sentence as one paragraph (includes Package Bytes...).

## Native transaction

Native transactions are optimized versions of common Sui operations. Each native transaction is semantically equivalent to a specific Move call but has a lower gas cost.

### Transfer

This transaction type transfers coins from the sender to the specified recipients.

In addition to the common metadata above, a publish transfer includes the following fields:
* Input: An object reference pointing to a mutable object owned by the sender. The object must be of type `Sui::Coin::Coin<T>` with arbitrary `T`--that is, any fungible token. The gas input object from above cannot also appear as an object input.
* Recipients: The addresses that will receive payments from this transfer. This list must be non-empty.
* Amounts: A list of unsigned integers encoding the amount that each recipient will receive. This list must be the same length as the recipients list. Each amount will be debited from the input object, wrapped in a freshly created coin object, and sent to the corresponding recipient address. The value of the input object must be greater than or equal to the sum of the amounts.

EDITORIAL NOTE: today's transfer sends the input object instead of debiting it + creating/transferring a new object. But that seems inconvenient because a user will not typically have an object with exactly the intended value. This means the typical "send money" flow will be two txes: (1) split an existing coin object to get a fresh one with the desired value, then (2) send the fresh coin. I think we should change transfer to work in the way described here, which combines this into a single transaction. I think we will still want a transfer that works like the current one as well, but I think it should operate on (e.g.) nonfungible token types instead once we have a standard for those, and should support sending multiple tokens to multiple recipients in a single transaction.

### Join

This transaction type combines several coin objects into one. It includes the following fields:

* Inputs: A list of unique object references pointing to mutable objects owned by the sender. The objects must all have the same type: `Sui::Coin::Coin<T>` with arbitrary `T`--that is, any fungible token. The list must contain at least two objects. All objects except the first one will be destroyed, and the new value of the first object will be its old value plus the sum of the value of all destroyed objects. The gas input object from above cannot also appear as an object input.

EDITORIAL NOTE: this does not exist yet, but I think it should

TODO: Either add to the list of one above or combine with the intro sentence as one paragraph (includes Inputs...).

## Further reading

* See the [Move Quick Start](move.md) to learn about smart contracts.
* Transactions take objects as input and produce objects as output--learn about the [objects](objects.md), their structure and attributes.
* Transactions are executed by Sui [authorities](authorities.md).
