# Regulated currency experiment

This project aims to implement a regulated currency problem. As the Move's ownership model (combined with Sui's flexibility in the global storage) is enhancing decentralization, implementing a centralized system is not a trivial task. Furthermore, making it regulated/centralized while keeping the same type level (eg not deeply wrapped and accessible) sounds almost impossible.

## Abstract

Regulated currency is a kind of Coin that is regulated by a set of validators. In the very simple case, validators can decide which address can make transfers and access their balances and which cannot.

- To implement a registry we'll use a shared object managed by a single admin (for simplicity's sake).
- For permission authentification, we'll tag every object with the address of the sender/owner.
- For authorizing transfers, a "locked" transfer container will be used; and to put "locked" money to the balance, one will need to authenticate the transaction through the registry.
