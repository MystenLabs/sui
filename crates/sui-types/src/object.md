# `Owner` enum

This document specifies how **`Owner`** values control object versioning, ordering, PTB input arguments, and permissions in Sui. It also describes the relation between `Owner` and the **Transfer‑to‑Object (TTO)** mechanism and the legal state‑transitions among owner kinds.

---

## 1 `Owner` variants and core properties

### 1.1 Properties implied by `Owner`ship

| Property | Definition |
|----------|------------|
| **Versioning** | How an object's `SequenceNumber` is determined. **Manual** means the caller must supply the version when using the object as input, and the version is committed to as part of the transaction certificate; **Automatic** means the validator assigns a version at scheduling time. |
| **Ordering** | Every transaction is sequenced by consensus before execution. **Owned** inputs are referenced at a fixed version; when transactions use the same version of an address-owned object, the first one in commit order locks that version after consensus and the others are rejected; **Consensus** inputs get their versions assigned by consensus at scheduling time. |
| **PTB input** | Indicates whether an object can be an input to a PTB, and if so how it is referenced.
| **Permissions** | The capability set the implied authorization function `A(sender,object)` grants to the transaction sender based on `Owner` (see below for more details): **read (r)** – borrow `&T`; **write (w)** – borrow `&mut T`; **delete (d)** – delete the object; **transfer (t)** – change ownership, wrap. |


### 1.2 Core properties of each variant

| Variant | Versioning model | Ordering | PTB input | Permissions |
|---------|-----------------|----------------|-----------------|--------------------|
| **`AddressOwner(addr)`** | Manual | Owned | `ImmOrOwnedObject` | only `addr` ⇒ **r w d t** |
| **`ObjectOwner(parent)`** | Automatic (via parent) | — | Not accepted directly; accessed through parent UID | Via parent |
| **`Shared`** | Automatic | Consensus | `SharedObject` | global **r w d** |
| **`Immutable`** | Manual (final) | Owned | `ImmOrOwnedObject` (read‑only) | global **r d** |
| **`ConsensusAddressOwner(addr)`** | Automatic | Consensus | `SharedObject` | only `addr` ⇒ **r w d t** |

## 2 Transfer‑to‑Object (TTO)

### 2.1 Eligibility matrix

| Role | Allowed `Owner` kinds | Notes |
|------|----------------------|--------------------|
| **Parent** | `AddressOwner`, `Shared`, `ConsensusAddressOwner`, `ObjectOwner` (all mutable owners) | Cannot receive from `Immutable` |
| **Child** | `AddressOwner` only | Transfer to an object ID same as you would an address |

### 2.2 Operational flow

1. Sender calls `transfer::{public_,}transfer(child,@parent_id)`; the child’s owner field simply changes to `AddressOwner(parent_id)`.  
2. To use the child later, a PTB passes `Receiving<Child>` plus `&mut parent.id`; `transfer::receive`.  
3. Child may be re‑transferred, wrapped, frozen, etc., subject to standard `AddressOwner` rules.

## 3 State‑transition rules

* **Shared** objects must be shared at creation time (otherwise `ESharedNonNewObject` is returned). A `Shared` object cannot transition to another `Owner`.
* **Immutable** objects cannot transition to another `Owner` once frozen.
* **AddressOwner**, **ObjectOwner**, and **ConsensusAddressOwner** objects can all freely transition between these three ownership types or be made **Immutable**.

## 4 Permissions and authorization 

The implied authorization function `A(sender, owner) → { r, w, d, t }` returns the capability set granted to `sender` over `object` with the given `Owner`:

* **read (r)** – may pass `&T` into an entry‑point.  
* **write (w)** – may pass `&mut T`.  
* **delete (d)** – may delete (move out) the object.  
* **transfer (t)** – may change ownership, wrap/unwrap, or upgrade.

Authorization rules based on `Owner` are applied only for objects used as input to a transaction. Additional custom authorization rules can also be implemented in application logic in Move, by the module that defines a type.

| Owner variant | Rule |
|---------------|------|
| `AddressOwner(a)` and `ConsensusAddressOwner(a)` | if `sender == a` ⇒ { r w d t } else ∅ |
| `ObjectOwner(_)` | Determined by the parent |
| `Shared` | { r w d } – transfer prohibited |
| `Immutable` | { r d } – no mutation or transfer |

---

## 5 Owned vs consensus inputs

All transactions are sequenced by consensus and executed after commit. A transaction whose inputs are all owned or immutable needs no version assignment: it runs once its referenced versions are available, and if two transactions use the same version of an address-owned object, only the first in commit order acquires the lock and executes. Any consensus input, such as a `Shared` or `ConsensusAddressOwner` object, makes the transaction wait for consensus to assign that object's version.
