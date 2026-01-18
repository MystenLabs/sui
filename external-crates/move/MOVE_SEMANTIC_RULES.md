# Move 2024 Semantic Rules for AI Coding Assistants

A guide to the semantic restrictions enforced by the Move 2024 compiler. Follow these rules to write valid Move code.

## 1. Implicit Imports and Aliases

Move 2024 automatically imports common modules and types. You don't need explicit `use` statements for these.

### Built-in Types (always available)

```move
bool, u8, u16, u32, u64, u128, u256, address, vector<T>
```

### Standard Library (auto-imported)

| Module/Type           | Available As |
| --------------------- | ------------ |
| `std::vector`         | `vector`     |
| `std::option`         | `option`     |
| `std::option::Option` | `Option`     |
| `std::internal`       | `internal`   |

### Sui Framework (auto-imported in Sui mode)

| Module/Type                  | Available As |
| ---------------------------- | ------------ |
| `sui::object`                | `object`     |
| `sui::transfer`              | `transfer`   |
| `sui::tx_context`            | `tx_context` |
| `sui::object::ID`            | `ID`         |
| `sui::object::UID`           | `UID`        |
| `sui::tx_context::TxContext` | `TxContext`  |

### Module-level Aliases

Within a module, these are automatically available:

- `Self` - alias for the current module
- All defined structs, enums, functions, and constants by their simple name

```move
module example::demo;

// No imports needed - these are implicit in Sui mode:
public struct MyObject has key {
    id: UID,           // UID is implicit (sui::object::UID)
    data: Option<u64>, // Option is implicit (std::option::Option)
}

public fun create(ctx: &mut TxContext): MyObject {  // TxContext is implicit
    MyObject {
        id: object::new(ctx),    // object module is implicit
        data: option::none(),    // option module is implicit
    }
}
```

## 2. Naming Rules

| Element          | Rule                             | Example                         |
| ---------------- | -------------------------------- | ------------------------------- |
| Variables        | Start with `a-z` or `_`          | `let count = 0;`                |
| Functions        | Start with `a-z`, no leading `_` | `public fun transfer()`         |
| Structs/Enums    | Start with `A-Z`                 | `public struct Coin { }`        |
| Constants        | Start with `A-Z`                 | `const MAX_SUPPLY: u64 = 1000;` |
| Type Parameters  | Start with `A-Z`                 | `public struct Box<T> { }`      |
| Macro Parameters | Must start with `$`              | `macro fun m($x: u64)`          |

## 2. Ability System

Every type in Move has a set of abilities that control how values can be used:

| Ability | Meaning                                       |
| ------- | --------------------------------------------- |
| `copy`  | Value can be copied                           |
| `drop`  | Value can be implicitly discarded             |
| `store` | Value can be stored in global storage         |
| `key`   | Value can be a top-level storage object (Sui) |

**Built-in type abilities:**

- Primitives (`u8`-`u256`, `bool`, `address`): `copy`, `drop`, `store`
- References (`&T`, `&mut T`): `copy`, `drop` only
- `vector<T>`: inherits from `T`

**Rules:**

- Struct abilities are declared: `public struct Coin has copy, drop { }`
- Type parameters can require abilities: `fun consume<T: drop>(x: T)`
- Types without `drop` MUST be explicitly consumed (cannot be ignored)
- Types without `copy` are moved on use (single ownership)

```move
module example::abilities;

public struct NoDrop { value: u64 }  // No abilities - cannot be dropped or copied

fun bad() {
    let x = NoDrop { value: 1 };
    // ERROR: x is not used and cannot be dropped
}

fun good() {
    let x = NoDrop { value: 1 };
    consume(x);  // x is explicitly consumed
}
```

## 3. Reference and Borrow Rules

**Immutable borrows (`&T`):**

- Multiple immutable borrows allowed simultaneously
- Cannot modify through immutable reference

**Mutable borrows (`&mut T`):**

- Only ONE mutable borrow at a time
- No other borrows (mutable or immutable) can coexist
- Variable must be declared `mut`: `let mut x = ...`

**Critical restrictions:**

- Cannot return references to local variables (dangling)
- Struct fields CANNOT contain references
- References have `copy` and `drop` but NOT `store`

```move
module example::refs;

// WRONG: Cannot store references in structs
public struct Bad { r: &u64 }

// WRONG: Dangling reference
fun bad(): &u64 {
    let x = 5;
    &x  // ERROR: x is dropped, reference dangles
}

// CORRECT: Borrow from parameter
fun good(x: &u64): &u64 { x }
```

## 4. Mutability Rules

Variables are immutable by default. Use `let mut` for mutable variables:

```move
let x = 5;
x = 6;  // ERROR: cannot mutate immutable variable

let mut y = 5;
y = 6;  // OK
```

To take a mutable borrow, the variable must be `mut`:

```move
let x = 5;
let r = &mut x;  // ERROR

let mut y = 5;
let r = &mut y;  // OK
```

## 5. Move vs Copy Semantics

```move
module example::ownership;

public struct Coin has copy, drop { value: u64 }
public struct NFT has drop { id: u64 }  // No copy

fun example() {
    let coin = Coin { value: 100 };
    let c2 = coin;      // Copied (has copy)
    let c3 = coin;      // Still valid

    let nft = NFT { id: 1 };
    let n2 = nft;       // Moved (no copy)
    let n3 = nft;       // ERROR: nft was moved
}
```

## 6. Visibility Rules

| Visibility        | Accessible From   |
| ----------------- | ----------------- |
| `public`          | Anywhere          |
| `public(package)` | Same package only |
| (none)            | Same module only  |

```move
module pkg::a;

public fun pub_fn() { }           // Anyone
public(package) fun pkg_fn() { }  // Same package
fun internal_fn() { }             // This module only
```

## 7. Type Restrictions

**Recursive types are forbidden:**

```move
// ERROR: Recursive type
public struct Node { next: Node }

// OK: Use Option or vector for indirection
public struct Node { next: Option<Node> }
```

**Phantom type parameters:**

- Declared with `phantom` keyword
- Cannot appear in field types (only in phantom positions)

```move
public struct Marker<phantom T> {}  // OK: T not used in fields
public struct Bad<phantom T> { value: T }  // ERROR: phantom T in field
```

## 8. Constant Restrictions

Constants can only have these types:

- Primitives: `u8`, `u16`, `u32`, `u64`, `u128`, `u256`, `bool`, `address`
- `vector` of primitives
- Byte strings

```move
const MAX: u64 = 100;               // OK
const BYTES: vector<u8> = b"hello"; // OK
const BAD: Coin = Coin { };         // ERROR: struct not allowed
```

Constant expressions cannot contain:

- Function calls
- Control flow (if/loops)
- References
- Non-constant values

## 9. Pattern Matching Rules

**Patterns must be exhaustive:**

```move
// ERROR: Non-exhaustive
match (opt) {
    Option::Some(x) => x,
    // Missing None case
}

// CORRECT
match (opt) {
    Option::Some(x) => x,
    Option::None => 0,
}
```

## 10. Function Arity

Type arguments and value arguments must match exactly:

```move
fun take_two<T>(a: T, b: T) { }

take_two(1);           // ERROR: too few arguments
take_two(1, 2, 3);     // ERROR: too many arguments
take_two<u64>(1, 2);   // OK
take_two(1, 2);        // OK (type inferred)
```

## 11. Common Errors and Fixes

| Error                | Cause                           | Fix                                         |
| -------------------- | ------------------------------- | ------------------------------------------- |
| "value without drop" | Type lacks `drop`, not consumed | Explicitly use or destroy the value         |
| "cannot copy"        | Type lacks `copy`               | Use `move` or add `copy` ability            |
| "invalid borrow"     | Borrowing moved value           | Borrow before move, or copy first           |
| "cannot mutate"      | Variable not `mut`              | Add `mut` to declaration                    |
| "dangling reference" | Returning local ref             | Return owned value or borrow param          |
| "recursive type"     | Self-referential struct         | Use `Option` or `vector` indirection        |
| "visibility"         | Calling private function        | Make function `public` or `public(package)` |

## 12. Quick Reference: Valid Patterns

```move
module example::demo;

// Struct with abilities
public struct Coin has copy, drop, store { value: u64 }

// Struct without copy (owned resource)
public struct NFT has key, store { id: UID, data: vector<u8> }

// Generic with ability constraint
public fun transfer<T: store>(obj: T) { ... }

// Mutable parameter
public fun increment(self: &mut Coin) {
    self.value = self.value + 1;
}

// Entry function (Sui)
public entry fun mint(ctx: &mut TxContext) { ... }
```

---

## 13. Sui-Specific Rules

These rules are enforced in addition to core Move 2024 semantics when compiling for Sui.

### 13.1 Object Rules

**Objects must have `id: UID` as first field:**

```move
// CORRECT: First field is id: UID
public struct MyObject has key {
    id: UID,
    data: u64,
}

// ERROR: Missing UID or wrong position
public struct Bad has key { data: u64 }
public struct AlsoBad has key { data: u64, id: UID }
```

**Enums cannot have `key` ability:**

```move
// ERROR: Enums cannot be objects
public enum MyEnum has key { A, B }
```

**Fresh UID required for object creation:**

```move
// CORRECT: UID from object::new()
let obj = MyObject {
    id: object::new(ctx),
    data: 0,
};

// ERROR: Reusing or passing UID from elsewhere
let obj = MyObject { id: some_other_uid, data: 0 };
```

### 13.2 `init` Function Rules

The `init` function is a special module initializer:

```move
module example::my_module;

public struct MY_MODULE has drop {}

// CORRECT init signatures:
fun init(ctx: &mut TxContext) { }
fun init(otw: MY_MODULE, ctx: &mut TxContext) { }
```

```move
// ERRORS:
public fun init(ctx: &mut TxContext) { }     // Must be private
entry fun init(ctx: &mut TxContext) { }      // Cannot be entry
fun init<T>(ctx: &mut TxContext) { }         // No type parameters
fun init(ctx: &mut TxContext): u64 { 0 }     // Must return unit
fun init(a: u64, b: u64, ctx: &mut TxContext) { }  // Max 2 params
```

**Rules:**

- Must be private (no visibility modifier)
- Cannot be `entry`
- No type parameters allowed
- Must return `()`
- Last parameter must be `&TxContext` or `&mut TxContext`
- Maximum 2 parameters (OTW + TxContext)
- Cannot be called directly (only at publish time)

### 13.3 One-Time Witness (OTW)

OTW is a struct named after the module (uppercase) used for one-time initialization:

```move
module example::my_coin;

// OTW: uppercase module name
public struct MY_COIN has drop {}

fun init(otw: MY_COIN, ctx: &mut TxContext) {
    // otw can only be received here, never constructed
}
```

**OTW Requirements:**

- Name must be uppercase version of module name
- Only `drop` ability (no `copy`, `store`, `key`)
- No type parameters
- No fields, or single `bool` field
- Cannot be manually constructed (passed by runtime to `init`)

### 13.4 Public / Entry Function Rules

Public and Entry functions can be called directly in PTBs:

```move
use std::string::String;

public fun do_something(
    obj: &mut MyObject,       // Objects by reference or value
    value: u64,               // Primitives by value
    object_id: ID,
    optional_object_id: Option<ID>,
    string_argument: String,
    ctx: &mut TxContext,      // TxContext last (optional)
) { }
```

**Valid parameter types:**

- Primitives (by value): `u8`-`u256`, `bool`, `address`
- Strings: `std::string::String`, `std::ascii::String`
- Object ID: `sui::object::ID`
- `Option<T>` where T is primitive
- `vector<T>` where T is primitive or object
- Objects (by reference or value): types with `key` ability
- `Receiving<T>` arguments

**Invalid parameters:**

- `&mut Clock` - must be `&Clock`
- `&mut Random` - must be `&Random`
- Non-object structs without `key`

**Return type:** Must have `drop` ability (or be unit `()`) for entry functions, and any type for public functions.

### 13.5 Transfer Rules

Private transfer functions are restricted:

```move
// These require T to be defined in the SAME module:
transfer::transfer(obj, recipient);
transfer::freeze_object(obj);
transfer::share_object(obj);

// For types with `store`, use public versions instead:
transfer::public_transfer(obj, recipient);
transfer::public_freeze_object(obj);
transfer::public_share_object(obj);
```

### 13.6 Event Rules

Events must use types defined in the current module:

```move
// CORRECT: MyEvent defined in this module
event::emit(MyEvent { value: 42 });

// ERROR: Cannot emit events with external types
event::emit(other_module::TheirEvent { });
```

### 13.7 Sui Linter Warnings

These are warnings (not errors) that indicate potential issues. Suppress with `#[allow(lint(<filter>))]`.

**Default Lints (enabled by default):**

| Filter                | Warning                                            | Issue                                                | Fix                                            |
| --------------------- | -------------------------------------------------- | ---------------------------------------------------- | ---------------------------------------------- |
| `share_owned`         | "possible owned object share"                      | Sharing object passed as parameter or from unpacking | Create fresh object and share in same function |
| `self_transfer`       | "non-composable transfer to sender"                | Transferring to `tx_context::sender()`               | Return object from function for composability  |
| `custom_state_change` | "potentially unenforceable custom transfer policy" | Custom transfer/share/freeze on types with `store`   | Use `public_transfer`/`public_share_object`    |
| `coin_field`          | "sub-optimal Coin field type"                      | `sui::coin::Coin` in struct field                    | Use `sui::balance::Balance` instead            |
| `freeze_wrapped`      | "attempting to freeze wrapped objects"             | Freezing struct containing nested `key` types        | Nested objects become inaccessible             |
| `collection_equality` | "possibly useless collections compare"             | Comparing `Table`, `Bag`, `VecMap` etc. with `==`    | Structural equality not checked                |
| `public_random`       | "Risky use of sui::random"                         | Public function taking `Random`/`RandomGenerator`    | Make function private or add access control    |
| `missing_key`         | "struct with id but missing key ability"           | Struct has `id: UID` field but no `key` ability      | Add `key` ability                              |
| `public_entry`        | "unnecessary entry on public function"             | `public entry fun` limits composability              | Remove `entry` or make function non-public     |

**Additional Lints (enable with `#[lint_allow(lint(all))]`):**

| Filter                  | Warning                                 | Issue                                   | Fix                                    |
| ----------------------- | --------------------------------------- | --------------------------------------- | -------------------------------------- |
| `freezing_capability`   | "freezing potential capability"         | Freezing types matching `*Cap*` pattern | Capabilities should not be frozen      |
| `prefer_mut_tx_context` | "prefer &mut TxContext over &TxContext" | Public function uses `&TxContext`       | Use `&mut TxContext` for upgradability |

### 13.8 Quick Reference: Sui Patterns

```move
module example::my_module;

// No imports needed - UID, TxContext, object, transfer are implicit in Sui mode

// Object (has key, first field is UID)
public struct MyObject has key, store {
    id: UID,
    value: u64,
}

// One-time witness
public struct MY_MODULE has drop {}

// Module initializer
fun init(_otw: MY_MODULE, _ctx: &mut TxContext) {
    // Called once at publish
}

// Entry function
entry fun create_and_keep(ctx: &mut TxContext) {
    let obj = MyObject {
        id: object::new(ctx),
        value: 0,
    };
    transfer::transfer(obj, ctx.sender());
}

// Public function which allows objects to be used in the transaction
public fun create(ctx: &mut TxContext): MyObject {
    MyObject {
        id: object::new(ctx),
        value: 0,
    }
}
```

---

## 14. Sui Storage Model

Sui uses an **object-centric storage model** where all persistent data is stored as objects. Each object has a unique ID and exists in one of several ownership states that determine how it can be accessed in transactions.

### 14.1 Owned Objects

Objects owned by a single address. Only the owner can use them in transactions.

```move
// Create and transfer to owner
let obj = MyObject { id: object::new(ctx), value: 0 };
transfer::transfer(obj, ctx.sender());
```

**Characteristics:**

- Fast path execution (no consensus required for single-owner transactions)
- Can be transferred to another address
- Can be converted to shared or immutable

### 14.2 Shared Objects

Objects accessible by any transaction. Require consensus for ordering.

```move
// CORRECT: Create and share in same function
let obj = MyObject { id: object::new(ctx), value: 0 };
transfer::share_object(obj);

// WRONG: Sharing object from parameter aborts at runtime
public fun bad(obj: MyObject) {
    transfer::share_object(obj);  // Lint warning: share_owned
}
```

**Characteristics:**

- Any address can read/write (if function allows)
- Requires consensus ordering (slower than owned)
- Cannot be transferred or made immutable once shared
- Use `&mut` reference in entry functions to modify
- **Must share freshly created objects** (not from parameters or unpacking)

### 14.3 Immutable Objects

Frozen objects that can never be modified. Anyone can read.

```move
// Freeze object permanently
transfer::freeze_object(obj);
```

**Characteristics:**

- Read-only forever (cannot be modified or deleted)
- Anyone can reference with `&T`
- No consensus needed (fast reads)
- Useful for configuration, constants, published packages

### 14.4 Wrapped Objects

Objects stored as fields inside other objects. Not directly accessible.

```move
public struct Wrapper has key {
    id: UID,
    inner: InnerObject,  // wrapped - not in global storage
}
```

**Characteristics:**

- No longer exists as independent object in storage
- Accessed only through parent object
- UID still exists but object is not addressable
- Can be "unwrapped" by extracting and transferring

### 14.5 Object Ownership Summary

| State         | Owner          | Consensus         | Mutable    | Use Case                            |
| ------------- | -------------- | ----------------- | ---------- | ----------------------------------- |
| **Owned**     | Single address | No                | Yes        | User assets, personal data          |
| **Shared**    | None (global)  | Yes               | Yes        | Shared state, AMM pools, registries |
| **Immutable** | None (frozen)  | No                | No         | Config, constants, packages         |
| **Wrapped**   | Parent object  | Depends on parent | Via parent | Composition, bundling               |

### 14.6 Storage State Transitions

```
                    ┌───────────────────────┐
        create ───▶ │    Freshly Created    │
                    └───────────────────────┘
                           │
           ┌───────────────┼───────────────┼───────────────┐
           ▼               ▼               ▼               ▼
    ┌────────────┐  ┌────────────┐  ┌────────────┐ ┌────────────┐
    │   Shared   │  │ Immutable  │  │  Wrapped   │ │   Owned    │
    └────────────┘  └────────────┘  └────────────┘ └────────────┘
```

**Rules:**

- Fresh → Shared: `transfer::share_object()` (irreversible)
- Fresh → Immutable: `transfer::freeze_object()` (irreversible)
- Fresh → Wrapped: Store as field in another object
- Wrapped → Owned: Extract and `transfer::transfer()`
- Shared/Immutable: Cannot change state once set

**Critical:**

- `share_owned` lint: warns when sharing an owned (transferred) object - changing its storage model
- Shared objects can be reshared (take by value, call `share_object` again)
- Shared objects can be deleted (take by value and destroy)
