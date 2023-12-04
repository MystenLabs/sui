# Programmable Transactions Blocks

Programmable transaction blocks are used to define all user transactions on Sui. These transactions allow a user to call multiple Move functions, manage their objects, and manage their coins in a single transaction--without publishing a new Move module! Additionally, the structure of programmable transaction blocks was designed with automation and transaction builders in mind. In other words, they are designed to be a lightweight and flexible way of generating transactions. That being said, more intricate programming patterns, such as loops, are not supported, and in those cases, a new Move module should be published.

Each programmable transaction block is consists of a block that is comprised of individual transaction commands (sometimes referred to themselves as transactions). Each transaction command is executed in order, and the results from a transaction command can be used in any subsequent transaction command. The effects, i.e. object modifications or transfers, of all transaction commands in a block are applied atomically at the end of the transaction, and if one transaction command fails, the entire block fails and no effects from the commands are applied.

This document will cover the semantics of the execution of the transaction commands. Note that it will assume familiarity with the Sui object model and the Move language. For more information on those topics, see the following documents: (TODO LINKS)

## Transaction Type

In this document, we will be looking at the two parts of a programmable transaction block that are relevant to the exectuion semantics. Other transaction information, such as the transaction sender or the gas limit, might be referenced but are not out of scope. The programmable transaction block consists of two components

- The inputs, `Vec<CallArg>`, is a vector of arguments, either objects or pure values, that can be used in the transaction commands. The objects are either owned by the sender or are shared/immutable objects. The pure values represent simple Move values, such as `u64` or `String` values, which can be constructed purely by their bytes.
- The commands, `Vec<Command>`, is a vector of transaction commands. The possible commands are:
  - `TransferObjects` sends multiple (1 or more) objects to a specified address.
  - `SplitCoins` splits off mutliple (1 or more) coins from a single coin. It can be any `sui::coin::Coin` object.
  - `MergeCoins` merges multiple (1 or more) coins into a single coin. Any `sui::coin::Coin` objects can be merged, as long as they are all of the same type.
  - `MakeMoveVec` creates a vector (potentially empty) of Move values. This is used primarily to construct vectors of Move values to be used as arguments to `MoveCall`.
  - `MoveCall` invokes either an `entry` or a `public` Move function in a published package.
  - `Publish` creates a new package and calls the `init` function of each module in the package.
  - `Upgrade` upgrades an existing package. The upgrade is gated by the `sui::package::UpgradeCap` for that package.

## Arguments and Results

Inputs and Results are the two types of values that can be used in transaction commands. Inputs are the values that are provided to the transaction block, and results are the values that are produced by the transaction block's commands. The inputs are either objects or simple Move values, and the results are arbitrary Move values (including objects).

The inputs and results can be seen as populating an array of values. For inputs, there is a single array, but for results, there is an array for each individual transaction command, creating a 2D-array of result values. These values can be accessed by borrowing (mutably or immutably), by copying (if the type permits), or by moving (which takes the value out of the array without re-indexing). First, we will look at the shape of each array, and then we will look at the semantics of each access type.

### Inputs

Input arguments to a programmable transaction block are broadly categorized as either objects or pure values. The direct implementation of these arguments is often obscured by transaction builders or SDKs. This section will describe information or data needed by the Sui network when specifying the list of inputs, `Vec<CallArg>`. Where each `CallArg` is either an object, `CallArg::Object(ObjectArg)`, which contains the necessary metadata to specify to object being used, or a pure value, `CallArg::Pure(PureArg)`, which contains the bytes of the value.

For object inputs, there is a different set of metadata needed differs depending on the type ownership of the object. The rules for authentication of these objects is described elsewhere (TODO LINK), but below is the actual data in the `ObjectArg` enum.

- If the object is owned by an address (or it is immutable), then `ObjectArg::ImmOrOwnedObject(ObjectRef)` is used. The `ObjectRef` is a triple `(ObjectID, SequenceNumber, ObjectDigest)` which respectively specifies the object's ID, it's version or sequence number, and the digest of the object's data.
- If an object is shared, then `Object::SharedObject { id: ObjectID, initial_shared_version: SequenceNumber, mutable: bool }` is used. Unlike `ImmOrOwnedObject`, a shared's objects version and digest are determined by the network's consensus protocol. The `initial_shared_version` is the version of the object when it was first shared, which is used by consensus when it has not yet seen a transaction with that object. While all shared objects _can_ be mutated, the `mutable` flag indicates whether the object will be used mutably in this transaction. In the case where the `mutable` flag is set to `false`, the object is read-only, and the system can schedule other read-only transactions in parallel.
- If the object is owned by another object, i.e. it was sent to an object's ID via the `TransferObjects` command or the `sui::transfer::transfer` function, then `ObjectArg::Receiving(ObjectRef)` is used. The data in the `ObjectRef` is the same as for the `ImmOrOwnedObject` case.

For pure inputs, the only data provided is the BCS (TODO Link) bytes. The bytes are not validated until the type is specified in a transaction command, e.g. in `MoveCall` or `MakeMoveVec`. Not all Move values can be constructed from BCS bytes. The following types are supported:

- All primitive types:
  - `u8`
  - `u16`
  - `u32`
  - `u64`
  - `u128`
  - `u256`
  - `bool`
  - `address`
- A string, either an ASCII string `std::ascii::String` or UTF8 string `std::string::String`. In either case, the bytes will be validated to be a valid string with the respective encoding.
- An object ID `sui::object::ID`.
- A vector, `vector<T>`, where `T` is a valid type for a pure input, checked recursively.
- An option, `std::option::Option<T>`, where `T` is a valid type for a pure input, checked recursively.

### Results

Each transaction command produces a (possibly empty) array of values. The type of the value can be any arbitrary Move type, so unlike inputs, the values are not limited to objects or pure values. The number of results generated and their types is specific to each transaction command. The specifics for each command can be found in the section for that command, but in summary:

- `MoveCall`: the number of results and their types are determined by the Move function being called. Note that Move functions that return references are not supported at this time.
- `SplitCoins`: produces (1 or more) coins from a single coin. The type of each coin is `sui::coin::Coin<T>` where the specific coin type `T` matches the coin being split.
- `Publish`: returns the upgrade capability, `sui::package::UpgradeCap` for the newly published package.
- `TransferObjects`, `MergeCoins`, and `Publish` do not produce any results (an empty result vector).

### Argument Structure and Usage

Each command takes `Argument`s, which specify the input or result being used. The usage (by-reference or by-value) is inferred based on the type of the argument and the expected argument of the command. First, let's look at the structure of the `Argument` enum.

- `Input(u16)` is an input argument, where the `u16` is the index of the input in the input vector.
  - For example, given an input vector of `[Object1, Object2, Pure1, Object3]`, `Object1` would be accessed with `Input(0)` and `Pure1` would be accessed with `Input(2)`.
- `GasCoin` is a special input argument representing the object for the GasCoin. It is kept separate from the other inputs because the gas coin is always present in each transaction and has special restrictions not present for other inputs. Additionally, the gas coin being separate makes its usage very explicit, which is helpful for sponsored transactions where the sponsor might not want the sender to use the gas coin for anything other than gas.
  - The gas coin cannot be taken by-value except with the `TransferObjects` command. If you need an owned version of the gas coin, you can first use `SplitCoins` to split off a single coin.
  - This limitation exists to make it easy for the remaining gas to be returned to the coin at the beginning of execution. In other words, if the gas coin was wrapped or deleted, then there would not be an obvious spot for the excess gas to be returned. See the execution section for more details.
- `NestedResult(u16, u16)` uses the value from a previous command. The first `u16` is the index of the command in the command vector, and the second `u16` is the index of the result in the result vector of that command.
  - For example, given a command vector of `[MoveCall1, MoveCall2, TransferObjects]` where `MoveCall2` has a result vector of `[Value1, Value2]`, `Value1` would be accessed with `NestedResult(1, 0)` and `Value2` would be accessed with `NestedResult(1, 1)`.
- `Result(u16)` is a special form of `NestedResult` where `Result(i)` is equivalent to `NestedResult(i, 0)`. However, this will error if the result array at index `i` is empty or has more than one value.
  - The intention of `Result` was to allow for accessing of the entire result array, but that is not yet supported at this time. So in general, `NestedResult` should be used instead of `Result`.

## Execution

For the execution of programmable transaxtion block: the input vector is populated by the input objects or pure value bytes. Then the commands are executed in order, and the results are stored in the result vector. Finally, the effects of the transaction are applied atomically. The following sections will describe each aspect of execution in greater detail.

### Start of Execution

At the beginning of execution, the programmable transaction block runtime takes the already loaded input objects and loads them into the input array. The objects are already verified by the network, checking rules like existence and valid ownership. The pure value bytes are also loaded into the array but not validated until usage.

The most important thing to note at this stage is the effects on the gas coin. At the beginning of execution, the maximum gas budget (in terms of `SUI`) is withdrawn from the gas coin. Any unused gas will be returned to the gas coin at the end of execution, even if the coin has changed owners!

### Executing a Command

Each command is then executed in order. First, let's look a the rules around arguments, which are shared by all commands.

#### Arguments

- Each argument can be used by-reference or by-value. The usage is based on the type of the argument and the type signature of the command.
  - If the signature expects an `&mut T`, the runtime checks the argument has type `T` and it is then mutably borrowed.
  - If the signature expects an `&T`, the runtime checks the argument has type `T` and it is then immutably borrowed.
  - If the signature expects an `T`, the runtime checks the argument has type `T` and it is copied if `T: copy` and moved otherwise.
    - Note that no object in Sui has `copy` because the unique ID field `sui::object::UID` present in all objects does not have the `copy` ability.
- The transaction fails if an argument is used in any form after being moved. There is no way to restore an argument to its position (its input or result index) after it is moved.
- If an argument is copied but does not have the `drop` ability, then the last usage is inferred to be a move. As a result, if an argument has `copy` and does not have `drop`, the last usage _must_ be by value. Otherwise, the transaction will fail because a value without `drop` has not been used.
- The borrowing of arguments has other rules to ensure unique safe usage of an argument by reference.
  - If an argument is mutably borrowed, there must be no outstanding borrows.
  - If an argument is immutably borrowed, there must be no outstanding _mutable_ borrows. Duplicate immutable borrows are allowed.
  - If an argument is moved, there must be no outstanding borrows. Moving a borrowed value would make those outstanding borrows unsafe.
  - If an argument is copied, there must be no outstanding _mutable_ borrows. It is safe and allowed to copy a value that is immutably borrowed.
- The `GasCoin` has special restrictions on being used by-value (moved). It can only be used by-value with the `TransferObjects` command.
- Shared objects also have restrictions on being used by-value. These restrictions exist to ensure that at the end of the transaction the shared object is either still shared or has been deleted. A shared object cannot be unshared, i.e. having the owner changed, and it cannot be wrapped.
  - A shared object marked as not `mutable`, that is it was marked as being used read-only, cannot be used by value.
  - A shared object cannot be transferred or frozen. These checks are _not_ done dynamically however. Only at the end of the transaction. For example, `TransferObjects` will succeed if passed a shared object, but at the end of execution, the transaction will fail.
  - A shared object can be wrapped and can become a dynamic field transiently, but by the end of the transaction, it must be re-shared or deleted.
- Pure values are not type checked until their usage.
  - When checking if a pure value has type `T`, it is checked whether `T` is a valid type for a pure value (see the list above). If it is, the bytes are then validated.
  - A pure value can be used be used with multiple types as long as the bytes are valid for each type. For example, a string could be used as an ASCII string `std::ascii::String` and as a UTF8 string `std::string::String`.
  - However, once the pure value is mutably borrowed, the type becomes fixed. And all future usages must be with that type.

#### `TransferObjects`

The command has the form `TransferObjects(ObjectArgs, AddressArg)` where `ObjectArgs` is a vector of objects and `AddressArg` is the address the objects are sent to.

- Each argument `ObjectArgs: Vec<Argument>` must be an object. However, the objects do not have the same type.
- The address argument `AddressArg: Argument` must be an address, which could come from a `Pure` input or a result.
- All arguments, objects and address, are taken by value.
- The command does not produce any results (an empty result vector).
- While the signature of this command cannot be expressed in Move, you can think of it roughly as having the signature `(vector<forall T: key + store. T>, address): ()` where `forall T: key + store. T` is indicating that the `vector` is a heterogenous vector of objects.

#### `SplitCoins`

The command has the form `SplitCoins(CoinArg, AmountArgs)` where `CoinArg` is the coin being split and `AmountArgs` is a vector of amounts to split off.

- When the transaction is signed, the network verifies that the AmountArgs is non-empty.
- The coin argument `CoinArg: Argument` must be a coin of type `sui::coin::Coin<T>` where `T` is the type of the coin being split. It can be any coin type and is not limited to `SUI` coins.
- The amount arguments `AmountArgs: Vec<Argument>` must be `u64` values, which could come from a `Pure` input or a result.
- The coin argument `CoinArg` is taken by mutable reference.
- The amount arguments `AmountArgs` are taken by value (copied).
- The result of the command is a vector of coins, `sui::coin::Coin<T>`. The coin type `T` is the same as the coin being split, and the number of results matches the number of arguments
- For a rough signature expressed in Move, it is similar to a function `<T: key + store>(coin: &mut sui::coin::Coin<T>, amounts: vector<u64>): vector<sui::coin::Coin<T>>` where the result `vector` is guaranteed to have the same length as the `amounts` vector.

#### `MergeCoins`

The command has the form `MergeCoins(CoinArg, ToMergeArgs)` where the `CoinArg` is the target coin in which the `ToMergeArgs` coins are merged into. In other words, we merge multiple coins (`ToMergeArgs`) into a single coin (`CoinArg`).

- When the transaction is signed, the network verifies that the AmountArgs is non-empty.
- The coin argument `CoinArg: Argument` must be a coin of type `sui::coin::Coin<T>` where `T` is the type of the coin being merged. It can be any coin type and is not limited to `SUI` coins.
- The coin arguments `ToMergeArgs: Vec<Argument>` must be `sui::coin::Coin<T>` values where the `T` is the same type as the `CoinArg`.
- The coin argument `CoinArg` is taken by mutable reference.
- The merge arguments `ToMergeArgs` are taken by value (moved).
- The command does not produce any results (an empty result vector).
- For a rough signature expressed in Move, it is similar to a function `<T: key + store>(coin: &mut sui::coin::Coin<T>, to_merge: vector<sui::coin::Coin<T>>): ()`

#### `MakeMoveVec`

The command has the form `MakeMoveVec(VecTypeOption, Args)` where `VecTypeOption` is an optional argument specifying the type of the elements in the vector being constructed and `Args` is a vector of arguments to be used as elements in the vector.

- When the transaction is signed, the network verifies that if that the type must be specified for an empty vector of `Args`.
- The type `VecTypeOption: Option<TypeTag>` is an optional argument specifying the type of the elements in the vector being constructed. The `TypeTag` is a Move type for the elements in the vector, i.e. the `T` in the produced `vector<T>`.
  - The type does not not have to be specified for an object vector--when `T: key`.
  - The type _must_ be specified if the type is not an object type or when the vector is empty.
- The arguments `Args: Vec<Argument>` are the elements of the vector. The arguments can be any type, including objects, pure values, or results from previous commands.
- The arguments `Args` are taken by value. Copied if `T: copy` and moved otherwise.
- The command produces a _single_ result of type `vector<T>`. The elements of the vector cannot then be accessed individually using `NestedResult`. Instead, the entire vector must be used as an argument to another command. If you wish to access the elements individually, you can use the `MoveCall` command and do so inside of Move code.
- While the signature of this command cannot be expressed in Move, you can think of it roughly as having the signature `(T...): vector<T>` where `T...` indicates a variadic number of arguments of type `T`.

#### `MoveCall`

This command has the form `MoveCall(Package, Module, Function, TypeArgs, Args)` where `Package::Module::Function` combine to specify the Move function being called, `TypeArgs` is a vector of type arguments to that function, and `Args` is a vector of arguments for the Move function.

- The package `Package: ObjectID` is the Object ID of the package containing the module being called.
- The module `Module: String` is the name of the module containing the function being called.
- The function `Function: String` is the name of the function being called.
- The type arguments `TypeArgs: Vec<TypeTag>` are the type arguments to the function being called. They must satisfy the constraints of the type parameters for the function.
- The arguments `Args: Vec<Argument>` are the arguments to the function being called. The arguments must be valid for the parameters as specified in the function's signature.
- Unlike the other commands, the usage of the arguments and the number of results are dynamic--in that they both depend on the signature of the Move function being called.

#### `Publish`

The command has the form `Publish(ModuleBytes, TransitiveDependencies)` where `ModuleBytes` are the bytes of the module being published and `TransitiveDependencies` is a vector of package Object ID dependencies to link against.

- When the transaction is signed, the network verifies that the `ModuleBytes` are not empty.
- The module bytes `ModuleBytes: Vec<Vec<u8>>` contain the bytes of the modules being published. Each element in the vector is a module.
- The transitive dependencies `TransitiveDependencies: Vec<ObjectID>` are the Object IDs of the packages that the new package depends on. While the modules themselves indicate the packages used as dependencies, the transitive object IDs must be provided to select the version of those packages. In other words, these object IDs are used to select the version of the packages marked as dependencies in the modules.
- After the modules are verified, the `init` function of each module is called in same order as the module byte vector `ModuleBytes`.
- The command produces a single result of type `sui::package::UpgradeCap`, which is the upgrade capability for the newly published package.

#### `Upgrade`

The command has the form `Upgrade(ModuleBytes, TransitiveDependencies, Package, UpgradeTicket)`, where the `Package` indicates the bject ID of the package being upgraded.  The `ModuleBytes` and `TransitiveDependencies` work similarly as the `Publish` command.

- For details on the `ModuleBytes` and `TransitiveDependencies`, see the `Publish` command. Note though, that no `init` functions are called for the upgraded modules.
- The `Package: ObjectID` is the Object ID of the package being upgraded. The package must exist and be the latest version.
- The `UpgradeTicket: sui::package::UpgradeTicket` is the upgrade ticket for the package being upgraded and is generated from the `sui::package::UpgradeCap`. The ticket is taken by value (moved).
- The command produces a single result type `sui::package::UpgradeReceipt` which provides proof for that upgrade.
- For more details on upgrades TODO link to package upgrade doc

### End of Execution

At the end of execution, the remaining values are checked and effects for the transaction are calculated.

- For inputs, the following checks are done:
  - Any remaining immutable or readonly input objects are skipped since no modifications have been made to them.
  - Any remaining mutable input objects are returned to their original owners--if they were shared they remain shared, if they were owned they remain owned.
  - Any remaining pure input values are dropped. Note that pure input values must have `copy` and `drop` since all permissible types for those values have `copy` and `drop`.
  - For any shared object we must also check that it has only been deleted or re-shared. Any other operation (wrap, transfer, freezing, etc) results in an error.
- For results, the following checks are done:
  - Any remaining result with the `drop` ability is dropped.
  - If the value has `copy` but not `drop`, it's last usage must have been by-value. In that way, it's last usage is treated as a move.
  - Otherwise, an error is given because there is an unused value without `drop`.
- Any remaining gas is returned to the gas coin, even if the owner has changed.
  - Note that since the gas coin can only be taken by-value with `TransferObjects`, it will not have been wrapped or deleted.

The total effects (which contain the created, mutated, and deleted objects) are then passed out of the execution layer and are applied by the Sui network.

## Examples

TODO
