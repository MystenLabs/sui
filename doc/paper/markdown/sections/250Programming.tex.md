[]{#sec:move label="sec:move"}

smart contracts are written in the Move[@move_white] language. Move is
safe and expressive, and its type system and data model naturally
support the parallel agreement/execution strategies that make scalable.
Move is an open-source programming language for building smart contracts
originally developed at Facebook for the Diem blockchain. The language
is platform-agnostic, and in addition to being adopted by , it has been
gaining popularity on other platforms (e.g., 0L, StarCoin).

In this section we will discuss the main features of the Move language
and explain how it is used to create and manage assets on . A more
thorough explanation of Move's features can be found in the Move
Programming Language book[^1] and more -specific Move content can be
found in the Developer Portal[^2], and a more formal description of Move
in the context of Sui can be found in
Section [\[sec:model\]](#sec:model){reference-type="ref"
reference="sec:model"}.

## Overview {#sec:move-overview}

's global state includes a pool of programmable objects created and
managed by Move *packages* that are collections of Move modules (see
Section [0.1.1](#sec:modules){reference-type="ref"
reference="sec:modules"} for details) containing Move functions and
types. Move packages themselves are also objects. Thus, objects can be
partitioned into two categories:

-   **Struct data values**: Typed data governed by Move modules. Each
    object is a struct value with fields that can contain primitive
    types (e.g. integers, addresses), other objects, and non-object
    structs.

-   **Package code values**: a set of related Move bytecode modules
    published as an atomic unit. Each module in a package can depend
    both on other modules in that package and on modules in previously
    published packages.

Objects can encode assets (e.g., fungible or non-fungible tokens),
*capabilities* granting the permission to call certain functions or
create other objects, "smart contracts" that manage other assets, and so
on--it's up to the programmer to decide. The Move code to declare a
custom object type looks like this:

``` {.Move basicstyle="\\scriptsize\\ttfamily" language="Move"}
struct Obj has key {
  id: VersionedID, // globally unique ID and version
  f: u64 // objects can have primitive fields
  g: OtherObj // fields can also store other objects
}
```

All structs representing objects (but not all Move struct values) must
have the field and the ability[^3] indicating that the value can be
stored in 's global object pool.

### Modules {#sec:modules}

A Move program is organized as a set of modules, each consisting of a
list of struct declarations and function declarations. A module can
import struct types from other modules and invoke functions declared by
other modules.

Values declared in one Move module can flow into another--e.g., module
in the example above could be defined in a different module than the
module defining . This is different from most smart contract languages,
which allow only unstructured bytes to flow across contract boundaries.
However, Move is able to support this because it provides encapsulation
features to help programmers write *robustly
safe* [@DBLP:journals/corr/abs-2110-05043] code. Specifically, Move's
type system ensures that a type like above can only be created,
destroyed, copied, read, and written by functions inside the module that
declares the type. This allows a module to enforce strong invariants on
its declared types that continue to hold even when they flow across
smart contract trust boundaries.

### Transactions and Entrypoints.

The global object pool is updated via transactions that can create,
destroy, read, and write objects. A transaction must take each existing
object it wishes to operate on as an input. In addition, a transaction
must include the versioned ID of a package object, the name of a module
and function inside that package, and arguments to the function
(including input objects). For example, to call the function

``` {.Move basicstyle="\\scriptsize\\ttfamily" language="Move"}
public fun entrypoint(
  o1: Obj, o2: &mut Obj, o3: &Obj, x: u64, ctx: &mut TxContext
) { ... }
```

a transaction must supply ID's for three distinct objects whose type is
and an integer to bind to . The is a special parameter filled in by the
runtime that contains the sender address and information required to
create new objects.

Inputs to an entrypoint (and more generally, to any Move function) can
be passed with different mutability permissions encoded in the type. An
input can be read, written, transferred, or destroyed. A input can only
be read or written, and a can only be read. The transaction sender must
be authorized to use each of the input objects with the specified
mutability permissions--see
Section [\[sec:owners\]](#sec:owners){reference-type="ref"
reference="sec:owners"} for more detail.

### Creating and Transferring Objects.

Programmers can create objects by using the passed into the entrypoint
to generate a fresh ID for the object:

``` {.Move basicstyle="\\scriptsize\\ttfamily" language="Move"}
public fun create_then_transfer(
  f: u64, g: OtherObj, o1: Obj, ctx: &mut TxContext
) {
  let o2 = Obj { id: TxContext::fresh_id(ctx), f, g };
  Transfer::transfer(o1, TxContext:sender());
  Transfer::transfer(o2, TxContext:sender());
}
```

This code takes two objects of type and as input, uses the first one and
the generated ID to create a new , and then transfers both objects to
the transaction sender. Once an object has been transferred, it flows
into the global object pool and cannot be accessed by code in the
remainder of the transaction. The module is part of the standard
library, which includes functions for transferring objects to user
addresses and to other objects.

We note that if the programmer code neglected to include one of the
calls, this code would be rejected by the Move type system. Move
enforces *resource safety* [@DBLP:journals/corr/abs-2004-05106]
protections to ensure that objects cannot be created without permission,
copied, or accidentally destroyed. Another example of resource safety
would be an attempt to transfer the same object twice, which would also
be rejected by the Move type system.

[^1]: <https://diem.github.io/move/>

[^2]: <https://github.com/MystenLabs/fastnft/blob/main/doc/SUMMARY.md>

[^3]: <https://diem.github.io/move/abilities.html>
