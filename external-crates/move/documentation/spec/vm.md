# Move VM Specification

Instantiation of a Move VM just initializes an instance of a `Loader`, that
is, a small set of empty tables (few instances of `HashMap` and `Vec` behind
`Mutex`). Initialization of a VM is reasonably inexpensive. The `Loader` is
effectively the code cache. The code cache has the lifetime of the VM. Code
is loaded at runtime when functions and scripts are executed. Once loaded,
modules and scripts are reused in their loaded form and ready to be executed
immediately. Loading code is expensive and the VM performs eager
loading. When execution starts, no more loading takes place, all code through
any possible control flow is ready to be executed and cached at load time.
Maybe, more importantly, the eager model guarantees that no runtime errors can
come from linking at runtime, and that a given invocation will not fail
loading/linking because of different code paths. The consistency of the
invocation is guaranteed before execution starts. Obviously runtime errors are
still possible and "expected".

This model fits typical blockchain requirements well:

* Validation uses only few functions published at genesis. Once loaded, code is
always fetched from the cache and immediately available.

* Execution is in the context of a given data view, a stable and immutable
view. As such code is stable too, and it is important to optimize the process
of loading. Also, transactions are reasonably homogeneous and reuse of code
leads to significant improvements in performance and stability.

The VM has an internal implementation for a data cache that relieves the client from an
important responsibility (data cache consistency). That abstraction is behind
a `Session` which is the only way to talk to the runtime.

The objective of a `Session` is to create and manage the data cache for a set
of invocations into the VM. It is also intended to return side effects in a
format that is suitable to the adapter.
A `Session` forwards calls to the `Runtime` which is where the logic and
implementation of the VM lives and starts.

### Code Cache

When loading a Module for the first time, the VM queries the data store for
the Module. That binary is deserialized, verified, loaded and cached by the
loader. Once loaded, a Module is never requested again for the lifetime of
that VM instance. Code is an immutable resource in the system.

The process of loading can be summarized through the following steps:

1. a binary—Module in a serialized form, `Vec<u8>`—is fetched from the data store.
This may require a network access
2. the binary is deserialized and verified
3. dependencies of the module are loaded (repeat 1.–4. for each dependency)
4. the module is linked to its dependencies (transformed in a representation
suitable for runtime) and cached by the loader.

So a reference to a loaded module does not perform any fetching from the
network, or verification, or transformations into runtime structures
(e.g. linking).

In a typical client, consistency of the code cache can be broken by a system transaction
that performs a hard upgrade, requiring the adapter to stop processing
transactions until a restart takes place. Other clients may have different
"code models" (e.g. some form of versioning).

Overall, a client holding an instance of a Move VM has to be aware of the
behavior of the code cache and provide data views (`DataStore`) that are
compatible with the loaded code. Moreover, a client is responsible to release
and instantiate a new VM when specific conditions may alter the consistency of
the code cache.

### Publishing

Clients may publish modules in the system by calling:

```rust
pub fn publish_module(
    &mut self,
    module: Vec<u8>,
    sender: AccountAddress,
    gas_status: &mut impl GasMeter,
) -> VMResult<()>;
```

The `module` is in a [serialized form](#Binary-Format) and the VM performs the
following steps:

* Deserialize the module: If the module does not deserialize, an error is
returned with a proper `StatusCode`.

* Check that the module address and the `sender` address are the same: This
check verifies that the publisher is the account that will eventually [hold
the module](#References-to-Data-and-Code). If the two addresses do not match, an
error with `StatusCode::MODULE_ADDRESS_DOES_NOT_MATCH_SENDER` is returned.

* Check that the module is not already published: Code is immutable in
Move. An attempt to overwrite an existing module results in an error with
`StatusCode::DUPLICATE_MODULE_NAME`.

* Verify loading: The VM performs [verification](#Verification) of the
module to prove correctness. However, neither the module nor any of its
dependencies are actually saved in the cache. The VM ensures that the module
will be loadable when a reference will be found. If a module would fail to
load an error with proper `StatusCode` is returned.

* Publish: The VM writes the serialized bytes of the module
with the [proper key](#References-to-Data-and-Code) to the storage.
After this step any reference to the
module is valid.

## Script Execution

The VM allows the execution of [scripts](#Binary-Format). A script is a
Move function declared in a `script` block that performs
calls into a Framework published on-chain to accomplish a
logical transaction. A script is not saved in storage and
it cannot be invoked by other scripts or modules.

```rust
pub fn execute_script(
    &mut self,
    script: Vec<u8>,
    ty_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    senders: Vec<AccountAddress>,
    gas_status: &mut impl GasMeter,
) -> VMResult<()>;
```

The `script` is specified in a [serialized form](#Binary-Format).
If the script is generic, the `ty_args` vector contains the `TypeTag`
values for the type arguments. The `signer` account addresses for the
script are specified in the `senders` vector. Any additional arguments
are provided in the `args` vector, where each argument is a BCS-serialized
vector of bytes. The VM
performs the following steps:

* Load the Script and the main function:

    - The `sha3_256` hash value of the `script` binary is computed.
    - The hash is used to access the script cache to see if the script was
      loaded. The hash is used for script identity.
    - If not in the cache the script is [loaded](#Loading). If loading fails,
      execution stops and an error with a proper `StatusCode` is returned.
    - The script main function is [checked against the
      type argument instantiation](#Verification) and if there are
      errors, execution stops and the error returned.

* Build the argument list: The first arguments are `Signer` values created by
the VM for the account addresses in the `senders` vector. Any other arguments
from the `args` vector are then checked against a whitelisted set of permitted
types and added to the arguments for the script.
The VM returns an error with `StatusCode::TYPE_MISMATCH` if
any of the types is not permitted.

* Execute the script: The VM invokes the interpreter to [execute the
script](#Interpreter). Any error during execution is returned, and the
transaction aborted. The VM returns whether execution succeeded or
failed.

## Script Function Execution

Script functions (in version 2 and later of the Move VM) are similar to scripts
except that the Move bytecode comes from a Move function with `script` visibility
in an on-chain module. The script function is specified by the module and function
name:

```rust
pub fn execute_script_function(
    &mut self,
    module: &ModuleId,
    function_name: &IdentStr,
    ty_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    senders: Vec<AccountAddress>,
    gas_status: &mut impl GasMeter,
) -> VMResult<()>;
```

Execution of script functions is similar to scripts. Instead of using the Move bytecodes
from a script, the script function is loaded from the on-chain module, and the Move VM
checks that it has `script` visibility. The rest of the script function execution is
the same as for scripts. If the function does not exist, execution fails with a
`FUNCTION_RESOLUTION_FAILURE` status code. If the function does not have `script` visibility,
it will fail with the `EXECUTE_SCRIPT_FUNCTION_CALLED_ON_NON_SCRIPT_VISIBLE` status code.

## Function Execution

The VM allows the execution of [any function in a module](#Binary-Format)
through a `ModuleId` and a function name. Function names are unique within a
module (no overloading), so the signature of the function is not
required. Argument checking is done by the [interpreter](#Interpreter).

The adapter uses this entry point to run specific system functions as
described in [validation](#Validation) and [execution](#Execution). This is a
very powerful entry point into the system given there are no visibility
checks. Clients would likely use this entry point internally (e.g., for
constructing a genesis state), or wrap and expose it with restrictions.

```rust
pub fn execute_function(
    &mut self,
    module: &ModuleId,
    function_name: &IdentStr,
    ty_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    gas_status: &mut impl GasMeter,
) -> VMResult<()>;
```

The VM performs the following steps:

* Load the function:

    - The specified `module` is first [loaded](#Loading).
      An error in loading halts execution and returns the error with a proper
      `StatusCode`.
    - The VM looks up the function in the module. Failure to resolve the
      function returns an error with a proper `StatusCode`.
    - Every type in the `ty_args` vector is [loaded](#Loading). An error
      in loading halts execution and returns the error with a proper `StatusCode`.
      Type arguments are checked against type parameters and an error returned
      if there is a mismatch (i.e., argument inconsistent with generic declaration).

* Build the argument list: Arguments are checked against a whitelisted set
of permitted types (_specify which types_). The VM returns an error with
`StatusCode::TYPE_MISMATCH` if any of the types is not permitted.

* Execute the function: The VM invokes the interpreter to [execute the
function](#Interpreter). Any error during execution aborts the interpreter
and returns the error. The VM returns whether execution succeeded or
failed.

## Binary Format

Modules and Scripts can only enter the VM in binary form, and Modules are
saved on chain in binary form. A Module is logically a collection of
functions and data structures. A Script is just an entry point, a single
function with arguments and no return value.

Modules can be thought as library or shared code, whereas Scripts can only
come in input with the Transaction.

Binaries are composed of headers and a set of tables. Some of
those tables are common to both Modules and Scripts, others specific to one or
the other. There is also data specific only to Modules or Scripts.

The binary format makes a heavy use of
[ULEB128](https://en.wikipedia.org/wiki/LEB128) to compress integers. Most of
the data in a binary is in the form of indices, and as such compression offers
an important saving. Integers, when used with no compression are in
[little-endian](https://en.wikipedia.org/wiki/Endianness) form.

Vectors are serialized with the size first, in ULEB128 form, followed by the
elements contiguously.

### Binary Header

Every binary starts with a header that has the following format:

* `Magic`: 4 bytes 0xA1, 0x1C, 0xEB, 0x0B (aka "A11CEB0B" or "AliceBob")
* `Version`: 4 byte little-endian unsigned integer
* `Table count`: number of tables in ULEB128 form. The current maximum number
of tables is contained in 1 byte, so this is effectively the count of tables in
one byte. Not all tables need to be present. Each kind of table can only be
present once; table repetitions are not allowed. Tables can be serialized in any
order.

### Table Headers

Following the binary header are the table headers. There are as many tables as
defined in "table count". Each table header
has the following format:

* `Table Kind`: 1 byte for the [kind of table](#Tables) that is serialized at
the location defined by the next 2 entries
* `Table Offset`: ULEB128 offset from the end of the table headers where the
table content starts
* `Table Length`: ULEB128 byte count of the table content

Tables must be contiguous to each other, starting from the end of the table
headers. There must not be any gap between the content of the tables. Table
content must not overlap.

### Tables

A `Table Kind` is 1 byte, and it is one of:

* `0x1`: `MODULE_HANDLES` - for both Modules and Scripts
* `0x2`: `STRUCT_HANDLES` - for both Modules and Scripts
* `0x3`: `FUNCTION_HANDLES` - for both Modules and Scriptss
* `0x4`: `FUNCTION_INSTANTIATIONS` - for both Modules and Scripts
* `0x5`: `SIGNATURES` - for both Modules and Scripts
* `0x6`: `CONSTANT_POOL` - for both Modules and Scripts
* `0x7`: `IDENTIFIERS` - for both Modules and Scripts
* `0x8`: `ADDRESS_IDENTIFIERS` - for both Modules and Scripts
* `0xA`: `STRUCT_DEFINITIONS` - only for Modules
* `0xB`: `STRUCT_DEF_INSTANTIATIONS` - only for Modules
* `0xC`: `FUNCTION_DEFINITIONS` - only for Modules
* `0xD`: `FIELD_HANDLES` - only for Modules
* `0xE`: `FIELD_INSTANTIATIONS` - only for Modules
* `0xF`: `FRIEND_DECLS` - only for Modules, version 2 and later

The formats of the tables are:

* `MODULE_HANDLES`: A `Module Handle` is a pair of indices that identify
the location of a module:

    * `address`: ULEB128 index into the `ADDRESS_IDENTIFIERS` table of
    the account under which the module is published
    * `name`: ULEB128 index into the `IDENTIFIERS` table of the name of the module

* `STRUCT_HANDLES`: A `Struct Handle` contains all the information to
uniquely identify a user type:

    * `module`: ULEB128 index in the `MODULE_HANDLES` table of the module
    where the struct is defined
    * `name`: ULEB128 index into the `IDENTIFIERS` table of the name of the struct
    * `nominal resource`: U8 bool defining whether the
    struct is a resource (true/1) or not (false/0)
    * `type parameters`: vector of [type parameter kinds](#Kinds) if the
    struct is generic, an empty vector otherwise:
        * `length`: ULEB128 length of the vector, effectively the number of type
        parameters for the generic struct
        * `kinds`: array of `length` U8 kind values; not present if length is 0

* `FUNCTION_HANDLES`: A `Function Handle` contains all the information to uniquely
identify a function:

    * `module`: ULEB128 index in the `MODULE_HANDLES` table of the module where
    the function is defined
    * `name`: ULEB128 index into the `IDENTIFIERS` table of the name of the function
    * `parameters`: ULEB128 index into the `SIGNATURES` table for the argument types
    of the function
    * `return`: ULEB128 index into the `SIGNATURES` table for the return types of the function
    * `type parameters`: vector of [type parameter kinds](#Kinds) if the function
    is generic, an empty vector otherwise:
        * `length`: ULEB128 length of the vector, effectively the number of type
        parameters for the generic function
        * `kinds`: array of `length` U8 kind values; not present if length is 0

* `FUNCTION_INSTANTIATIONS`: A `Function Instantiation` describes the
instantation of a generic function. Function Instantiation can be full or
partial. E.g., given a generic function `f<K, V>()` a full instantiation would
be `f<U8, Bool>()` whereas a partial instantiation would be `f<U8, Z>()` where
`Z` is a type parameter in a given context (typically another function
`g<Z>()`).

    * `function handle`: ULEB128 index into the `FUNCTION_HANDLES` table of the
    generic function for this instantiation (e.g., `f<K, W>()`)
    * `instantiation`: ULEB128 index into the `SIGNATURES` table for the
    instantiation of the function

* `SIGNATURES`: The set of signatures in this binary. A signature is a
vector of [Signature Tokens](#SignatureTokens), so every signature will carry
the length (in ULEB128 form) followed by the Signature Tokens.

* `CONSTANT_POOL`: The set of constants in the binary. A constant is a
copyable primitive value or a vector of vectors of primitives. Constants
cannot be user types. Constants are serialized according to the rule defined
in [Move Values](#Move-Values) and stored in the table in serialized form. A
constant in the constant pool has the following entries:

    * `type`: the [Signature Token](#SignatureTokens) (type) of the value that follows
    * `length`: the length of the serialized value in bytes
    * `value`: the serialized value

* `IDENTIFIERS`: The set of identifiers in this binary. Identifiers are
vectors of chars. Their format is the length of the vector in ULEB128 form
followed by the chars. An identifier can only have characters in the ASCII set
and specifically: must start with a letter or '\_', followed by a letter, '\_'
or digit

* `ADDRESS_IDENTIFIERS`: The set of addresses used in ModuleHandles.
Addresses are fixed size so they are stored contiguously in this table.

* `STRUCT_DEFINITIONS`: The structs or user types defined in the binary. A
struct definition contains the following fields:

    * `struct_handle`: ULEB128 index in the `STRUCT_HANDLES` table for the
    handle of this definition
    * `field_information`: Field Information provides information about the
    fields of the struct or whether the struct is native

        * `tag`: 1 byte, either `0x1` if the struct is native, or `0x2` if the struct
        contains fields, in which case it is followed by:
        * `field count`: ULEB128 number of fields for this struct
        * `fields`: a field count of

            * `name`: ULEB128 index in the `IDENTIFIERS` table containing the
            name of the field
            * `field type`: [SignatureToken](#SignatureTokens) - the type of
            the field

* `STRUCT_DEF_INSTANTIATIONS`: the set of instantiation for any given
generic struct. It contains the following fields:

    * `struct handle`: ULEB128 index into the `STRUCT_HANDLES` table of the
    generic struct for this instantiation (e.g., `struct X<T>`)
    * `instantiation`: ULEB128 index into the `SIGNATURES` table for the
    instantiation of the struct. The instantiation can be either partial or complete
    (e.g., `X<U64>` or `X<Z>` when inside another generic function or generic struct
    with type parameter `Z`)

* `FUNCTION_DEFINITIONS`: the set of functions defined in this binary. A
function definition contains the following fields:

    * `function_handle`: ULEB128 index in the `FUNCTION_HANDLES` table for
    the handle of this definition
    * `visibility`: 1 byte for the function visibility (only used in version 2 and later)

        * `0x0` if the function is private to the Module
        * `0x1` if the function is public and thus visible outside this module
        * `0x2` for a `script` function
        * `0x3` if the function is private but also visible to `friend` modules

    * `flags`: 1 byte:

        * `0x0` if the function is private to the Module (version 1 only)
        * `0x1` if the function is public and thus visible outside this module (version 1 only)
        * `0x2` if the function is native, not implemented in Move

    * `acquires_global_resources`: resources accessed by this function

        * `length`: ULEB128 length of the vector, number of resources
        acquired by this function
        * `resources`: array of `length` ULEB128 indices into the `STRUCT_DEFS` table,
        for the resources acquired by this function

    * `code_unit`: if the function is not native, the code unit follows:

        * `locals`: ULEB128 index into the `SIGNATURES` table for the types
        of the locals of the function
        * `code`: vector of [Bytecodes](#Bytecodes), the body of this function

            * `length`: the count of bytecodes the follows
            * `bytecodes`: Bytecodes, they are variable size

* `FIELD_HANDLES`: the set of fields accessed in code. A field handle is
composed by the following fields:

    * `owner`: ULEB128 index into the `STRUCT_DEFS` table of the type that owns the field
    * `index`: ULEB128 position of the field in the vector of fields of the `owner`

* `FIELD_INSTANTIATIONS`: the set of generic fields accessed in code. A
field instantiation is a pair of indices:

    * `field_handle`: ULEB128 index into the `FIELD_HANDLES` table for the generic field
    * `instantiation`: ULEB128 index into the `SIGNATURES` table for the instantiation of
    the type that owns the field

* `FRIEND_DECLS`: the set of declared friend modules with the following for each one:

    * `address`: ULEB128 index into the `ADDRESS_IDENTIFIERS` table of
    the account under which the module is published
    * `name`: ULEB128 index into the `IDENTIFIERS` table of the name of the module

### Kinds

A `Type Parameter Kind` is 1 byte, and it is one of:

* `0x1`: `ALL` - the type parameter can be substituted by either a resource, or a copyable type
* `0x2`: `COPYABLE` - the type parameter must be substituted by a copyable type
* `0x3`: `RESOURCE` - the type parameter must be substituted by a resource type

### SignatureTokens

A `SignatureToken` is 1 byte, and it is one of:

* `0x1`: `BOOL` - a boolean
* `0x2`: `U8` - a U8 (byte)
* `0x3`: `U64` - a 64-bit unsigned integer
* `0x4`: `U128` - a 128-bit unsigned integer
* `0x5`: `ADDRESS` - an `AccountAddress` in the chain, may be a 16, 20, or 32 byte value
* `0x6`: `REFERENCE` - a reference; must be followed by another SignatureToken
representing the type referenced
* `0x7`: `MUTABLE_REFERENCE` - a mutable reference; must be followed by another
SignatureToken representing the type referenced
* `0x8`: `STRUCT` - a structure; must be followed by the index into the
`STRUCT_HANDLES` table describing the type. That index is in ULEB128 form
* `0x9`: `TYPE_PARAMETER` - a type parameter of a generic struct or a generic
function; must be followed by the index into the type parameters vector of its container.
The index is in ULEB128 form
* `0xA`: `VECTOR` - a vector - must be followed by another SignatureToken
representing the type of the vector
* `0xB`: `STRUCT_INST` - a struct instantiation; must be followed by an index
into the `STRUCT_HANDLES` table for the generic type of the instantiation, and a
vector describing the substitution types, that is, a vector of SignatureTokens
* `0xC`: `SIGNER` - a signer type, which is a special type for the VM
representing the "entity" that signed the transaction. Signer is a resource type
* `0xD`: `U16` - a 16-bit unsigned integer
* `0xE`: `U32` - a 32-bit unsigned integer
* `0xF`: `U256` - a 256-bit unsigned integer

Signature tokens examples:

* `u8, u128` -> `0x2 0x2 0x4` - size(`0x2`), U8(`0x2`), u128(`0x4`)
* `u8, u128, A` where A is a struct -> `0x3 0x2 0x4 0x8 0x10` - size(`0x3`),
U8(`0x2`), u128(`0x4`), Struct::A
(`0x8 0x10` assuming the struct is in the `STRUCT_HANDLES` table at position `0x10`)
* `vector<address>, &A` where A is a struct -> `0x2 0xA 0x5 0x8 0x10` - size(`0x2`),
vector<address>(`0xA 0x5`), &Struct::A
(`0x6 0x8 0x10` assuming the struct is in the `STRUCT_HANDLES` table at position `0x10`)
* `vector<A>, &A<B>` where A and B are a struct ->
`0x2 0xA 0x8 0x10 0x6 0xB 0x10 0x1 0x8 0x11` -
size(`0x2`), vector\<A\>(`0xA 0x8 0x10`),
&Struct::A\<Struct::B\> (`0x6` &, `0xB 0x10` A<\_>, `0x1 0x8 0x11` B type
instantiation; assuming the struct are in the `STRUCT_HANDLES` table at position
`0x10` and `0x11` respectively)

### Bytecodes

Bytecodes are variable size instructions for the Move VM. Bytecodes are
composed by opcodes (1 byte) followed by a possible payload which depends on
the specific opcode and specified in "()" below:

* `0x01`: `POP`
* `0x02`: `RET`
* `0x03`: `BR_TRUE(offset)` - offset is in ULEB128 form, and it is the target
offset in the code stream from the beginning of the code stream
* `0x04`: `BR_FALSE(offset)` - offset is in ULEB128 form, and it is the
target offset in the code stream from the beginning of the code stream
* `0x05`: `BRANCH(offset)` - offset is in ULEB128 form, and it is the target
offset in the code stream from the beginning of the code stream
* `0x06`: `LD_U64(value)` - value is a U64 in little-endian form
* `0x07`: `LD_CONST(index)` - index is in ULEB128 form, and it is an index
in the `CONSTANT_POOL` table
* `0x08`: `LD_TRUE`
* `0x09`: `LD_FALSE`
* `0x0A`: `COPY_LOC(index)` - index is in ULEB128 form, and it is an index
referring to either an argument or a local of the function. From a bytecode
perspective arguments and locals lengths are added and the index must be in that
range. If index is less than the length of arguments it refers to one of the
arguments otherwise it refers to one of the locals
* `0x0B`: `MOVE_LOC(index)` - index is in ULEB128 form, and it is an index
referring to either an argument or a local of the function. From a bytecode
perspective arguments and locals lengths are added and the index must be in that
range. If index is less than the length of arguments it refers to one of the
arguments otherwise it refers to one of the locals
* `0x0C`: `ST_LOC(index)` - index is in ULEB128 form, and it is an index
referring to either an argument or a local of the function. From a bytecode
perspective arguments and locals lengths are added and the index must be in that
range. If index is less than the length of arguments it refers to one of the
arguments otherwise it refers to one of the locals
* `0x0D`: `MUT_BORROW_LOC(index)` - index is in ULEB128 form, and it is an
index referring to either an argument or a local of the function. From a
bytecode perspective arguments and locals lengths are added and the index must
be in that range. If index is less than the length of arguments it refers to one
of the arguments otherwise it refers to one of the locals
* `0x0E`: `IMM_BORROW_LOC(index)` - index is in ULEB128 form, and it is an
index referring to either an argument or a local of the function. From a
bytecode perspective arguments and locals lengths are added and the index must
be in that range. If index is less than the length of arguments it refers to one
of the arguments otherwise it refers to one of the locals
* `0x0F`: `MUT_BORROW_FIELD(index)` - index is in ULEB128 form, and it is an
index in the `FIELD_HANDLES` table
* `0x10`: `IMM_BORROW_FIELD(index)` - index is in ULEB128 form, and it is an
index in the `FIELD_HANDLES` table
* `0x11`: `CALL(index)` - index is in ULEB128 form, and it is an index in the
`FUNCTION_HANDLES` table
* `0x12`: `PACK(index)` - index is in ULEB128 form, and it is an index in the
`STRUCT_DEFINITIONS` table
* `0x13`: `UNPACK(index)` - index is in ULEB128 form, and it is an index in
the `STRUCT_DEFINITIONS` table
* `0x14`: `READ_REF`
* `0x15`: `WRITE_REF`
* `0x16`: `ADD`
* `0x17`: `SUB`
* `0x18`: `MUL`
* `0x19`: `MOD`
* `0x1A`: `DIV`
* `0x1B`: `BIT_OR`
* `0x1C`: `BIT_AND`
* `0x1D`: `XOR`
* `0x1E`: `OR`
* `0x1F`: `AND`
* `0x20`: `NOT`
* `0x21`: `EQ`
* `0x22`: `NEQ`
* `0x23`: `LT`
* `0x24`: `GT`
* `0x25`: `LE`
* `0x26`: `GE`
* `0x27`: `ABORT`
* `0x28`: `NOP`
* `0x29`: `EXISTS(index)` - index is in ULEB128 form, and it is an index in
the `STRUCT_DEFINITIONS` table
* `0x2A`: `MUT_BORROW_GLOBAL(index)` - index is in ULEB128 form, and it is
an index in the `STRUCT_DEFINITIONS` table
* `0x2B`: `IMM_BORROW_GLOBAL(index)` - index is in ULEB128 form, and it is
an index in the `STRUCT_DEFINITIONS` table
* `0x2C`: `MOVE_FROM(index)` - index is in ULEB128 form, and it is an index
in the `STRUCT_DEFINITIONS` table
* `0x2D`: `MOVE_TO(index)` - index is in ULEB128 form, and it is an index
in the `STRUCT_DEFINITIONS` table
* `0x2E`: `FREEZE_REF`
* `0x2F`: `SHL`
* `0x30`: `SHR`
* `0x31`: `LD_U8(value)` - value is a U8
* `0x32`: `LD_U128(value)` - value is a U128 in little-endian form
* `0x33`: `CAST_U8`
* `0x34`: `CAST_U64`
* `0x35`: `CAST_U128`
* `0x36`: `MUT_BORROW_FIELD_GENERIC(index)` - index is in ULEB128 form,
and it is an index in the `FIELD_INSTANTIATIONS` table
* `0x37`: `IMM_BORROW_FIELD_GENERIC(index)` - index is in ULEB128 form,
and it is an index in the `FIELD_INSTANTIATIONS` table
* `0x38`: `CALL_GENERIC(index)` - index is in ULEB128 form, and it is an
index in the `FUNCTION_INSTANTIATIONS` table
* `0x39`: `PACK_GENERIC(index)` - index is in ULEB128 form, and it is an
index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3A`: `UNPACK_GENERIC(index)` - index is in ULEB128 form, and it is an
index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3B`: `EXISTS_GENERIC(index)` - index is in ULEB128 form, and it is an
index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3C`: `MUT_BORROW_GLOBAL_GENERIC(index)` - index is in ULEB128 form,
and it is an index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3D`: `IMM_BORROW_GLOBAL_GENERIC(index)` - index is in ULEB128 form,
and it is an index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3E`: `MOVE_FROM_GENERIC(index)` - index is in ULEB128 form, and it
is an index in the `STRUCT_DEF_INSTANTIATIONS` table
* `0x3F`: `MOVE_TO_GENERIC(index)` - index is in ULEB128 form, and it is
an index in the `STRUCT_DEF_INSTANTIATIONS` table

### Module Specific Data

A binary for a Module contains an index in ULEB128 form as its last
entry. That is after all tables. That index points to the ModuleHandle table
and it is the self module. It is where the module is stored, and a
specification of which one of the Modules in the `MODULE_HANDLES` tables is the
self one.

### Script Specific Data

A Script does not have a `FUNCTION_DEFINITIONS` table, and the entry point is
explicitly described in the following entries, at the end of a Script
Binary, in the order below:

* `type parameters`: if the script entry point is generic, the number and
kind of the type parameters is in this vector.

    * `length`: ULEB128 length of the vector, effectively the number of
    type parameters for the generic entry point. 0 if the script is not generic
    * `kinds`: array of `length` U8 [kind](#Kinds) values, not present
    if length is 0

* `parameters`: ULEB128 index into the `SIGNATURES` table for the argument
types of the entry point

* `code`: vector of [Bytecodes](#Bytecodes), the body of this function
    * `length`: the count of bytecodes
    * `bytecodes`: Bytecodes contiguously serialized, they are variable size
