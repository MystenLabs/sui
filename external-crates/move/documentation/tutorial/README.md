# Move Tutorial

Welcome to the Move Tutorial! In this tutorial, we are going to go through some steps of developing Move code
including design, implementation, unit testing and formal verification of Move modules.

There are nine steps in total:

- [Step 0: Installation](#Step0)
- [Step 1: Writing my first Move module](#Step1)
- [Step 2: Adding unit tests to my first Move module](#Step2)
- [Step 3: Designing my `BasicCoin` module](#Step3)
- [Step 4: Implementing my `BasicCoin` module](#Step4)
- [Step 5: Adding and using unit tests with the `BasicCoin` module](#Step5)
- [Step 6: Making my `BasicCoin` module generic](#Step6)
- [Step 7: Use the Move prover](#Step7)
- [Step 8: Writing formal specifications for the `BasicCoin` module](#Step8)

Each step is designed to be self-contained in the corresponding `step_x` folder. For example, if you would
like to skip the contents in step 1 through 4, feel free to jump to step 5 since all the code we have written
before step 5 will be in `step_5` folder. At the end of some steps, we also include
additional material on more advanced topics.

Now let's get started!

## Step 0: Installation<span id="Step0"><span>

If you haven't already, open your terminal and clone [the Move repository](https://github.com/move-language/move):

```bash
git clone https://github.com/move-language/move.git
```

Go to the `move` directory and run the `dev_setup.sh` script:

```bash
cd move
./scripts/dev_setup.sh -ypt
```

Follow the script's prompts in order to install all of Move's dependencies.

The script adds environment variable definitions to your `~/.profile` file.
Include them by running this command:

```bash
source ~/.profile
````

Next, install Move's command-line tool by running this commands:

```bash
cargo install --path language/tools/move-cli
```

You can check that it is working by running the following command:

```bash
move --help
```

You should see something like this along with a list and description of a
number of commands:

```
move-cli 0.1.0
Diem Association <opensource@diem.com>
MoveCLI is the CLI that will be executed by the `move-cli` command The `cmd` argument is added here
rather than in `Move` to make it easier for other crates to extend `move-cli`

USAGE:
    move [OPTIONS] <SUBCOMMAND>

OPTIONS:
        --abi                          Generate ABIs for packages
...
```

If you want to find what commands are available and what they do, running
a command or subcommand with the `--help` flag will print documentation.

Before running the next steps, `cd` to the tutorial directory:
```bash
cd <path_to_move>/language/documentation/tutorial
```


<details>
<summary>Visual Studio Code Move support</summary>
There is official Move support for Visual Studio Code. You need to install
the move analyzer first:

```bash
cargo install --path language/move-analyzer
```

Now you can install the VS extension by opening VS Code, searching for the "move-analyzer" in
the Extension Pane, and installing it. More detailed instructions can be found
in the extension's [README](https://github.com/move-language/move/tree/main/language/move-analyzer/editors/code).
</details>


## Step 1: Writing my first Move module<span id="Step1"><span>

Change directory into the [`step_1/BasicCoin`](./step_1/BasicCoin) directory.
You should see a directory called `sources` -- this is the place where all
the Move code for this package lives. You should also see a
`Move.toml` file as well. This file specifies dependencies and other information about
the package; if you're familiar with Rust and Cargo, the `Move.toml` file
is similar to the `Cargo.toml` file, and the `sources` directory similar to
the `src` directory.

Let's take a look at some Move code! Open up
[`sources/FirstModule.move`](./step_1/BasicCoin/sources/FirstModule.move) in
your editor of choice. The first thing you'll see is this:

```
// sources/FirstModule.move
module 0xCAFE::BasicCoin {
    ...
}
```

This is defining a Move
[module](https://move-language.github.io/move/modules-and-scripts.html). Modules are the
building blocks of Move code, and are defined with a specific address -- the
address that the module can be published under. In this case, the `BasicCoin`
module can only be published under `0xCAFE`.

Let's now take a look at the next part of this file where we define a
[struct](https://move-language.github.io/move/structs-and-resources.html)
to represent a `Coin` with a given `value`:

```
module 0xCAFE::BasicCoin {
    struct Coin has key {
        value: u64,
    }
    ...
}
```

Looking at the rest of the file, we see a function definition that creates a `Coin` struct and stores it under an account:

```
module 0xCAFE::BasicCoin {
    struct Coin has key {
        value: u64,
    }

    public fun mint(account: signer, value: u64) {
        move_to(&account, Coin { value })
    }
}
```

Let's take a look at this function and what it's saying:
* It takes a [`signer`](https://move-language.github.io/move/signer.html) -- an
  unforgeable token that represents control over a particular address, and
  a `value` to mint.
* It creates a `Coin` with the given value and stores it under the
  `account` using the `move_to` operator.

Let's make sure it builds! This can be done with the `build` command from within the package folder ([`step_1/BasicCoin`](./step_1/BasicCoin/)):

```bash
move build
```

<details>
<summary>Advanced concepts and references</summary>

* You can create an empty Move package by calling:
    ```bash
    move new <pkg_name>
    ```
* Move code can also live a number of other places.  More information on the
  Move package system can be found in the [Move
  book](https://move-language.github.io/move/packages.html)
* More information on the `Move.toml` file can be found in the [package section of the Move book](https://move-language.github.io/move/packages.html#movetoml).
* Move also supports the idea of [named
  addresses](https://move-language.github.io/move/address.html#named-addresses), Named
  addresses are a way to parametrize Move source code so that you can compile
  the module using different values for `NamedAddr` to get different bytecode
  that you can deploy, depending on what address(es) you control. They are used quite frequently, and can be defined in the `Move.toml` file in the `[addresses]` section, e.g.,
    ```
    [addresses]
    SomeNamedAddress = "0xC0FFEE"
    ```
* [Structures](https://move-language.github.io/move/structs-and-resources.html) in Move
  can be given different
  [abilities](https://move-language.github.io/move/abilities.html) that describe what
  can be done with that type. There are four different abilities:
    - `copy`: Allows values of types with this ability to be copied.
    - `drop`: Allows values of types with this ability to be popped/dropped.
    - `store`: Allows values of types with this ability to exist inside a struct in global storage.
    - `key`: Allows the type to serve as a key for global storage operations.

    So in the `BasicCoin` module we are saying that the `Coin` struct can be used as a key
    in global storage and, because it has no other abilities, it cannot be
    copied, dropped, or stored as a non-key value in storage. So you can't copy
    coins, and you also can't lose coins by accident!
* [Functions](https://move-language.github.io/move/functions.html) are default
    private, and can also be `public`,
    [`public(friend)`](https://move-language.github.io/move/friends.html), or
    `public(script)`. The last of these states that this function can be
    called from a transaction script. `public(script)` functions can also be
    called by other `public(script)` functions.
* `move_to` is one of the [five different global storage
  operators](https://move-language.github.io/move/global-storage-operators.html).
</details>

## Step 2: Adding unit tests to my first Move module<span id="Step2"><span>

Now that we've taken a look at our first Move module, we'll take a look at a
test to make sure minting works the way we expect it to by changing directory
to [`step_2/BasicCoin`](./step_2/BasicCoin).  Unit tests in Move are similar to
unit tests in Rust if you're familiar with them -- tests are annotated with
`#[test]` and written like normal Move functions.

You can run the tests with the `move test` command:

```bash
move test
```

Let's now take a look at the contents of the [`FirstModule.move`
file](./step_2/BasicCoin/sources/FirstModule.move). The first new thing you'll
see is this test:

```
module 0xCAFE::BasicCoin {
    ...
    // Declare a unit test. It takes a signer called `account` with an
    // address value of `0xC0FFEE`.
    #[test(account = @0xC0FFEE)]
    fun test_mint_10(account: signer) acquires Coin {
        let addr = 0x1::signer::address_of(&account);
        mint(account, 10);
        // Make sure there is a `Coin` resource under `addr` with a value of `10`.
        // We can access this resource and its value since we are in the
        // same module that defined the `Coin` resource.
        assert!(borrow_global<Coin>(addr).value == 10, 0);
    }
}
```

This is declaring a unit test called `test_mint_10` that mints a `Coin` struct
under the `account` with a `value` of `10`. It is then checking that the minted
coin in storage has the value that is expected with the `assert!` call. If the
assertion fails the unit test will fail.

<details>
<summary>Advanced concepts and exercises</summary>

* There are a number of test-related annotations that are worth exploring, they
  can be found
  [here](https://github.com/move-language/move/blob/main/language/changes/4-unit-testing.md#testing-annotations-their-meaning-and-usage).
  You'll see some of these used in Step 5.
* Before running unit tests, you'll always need to add a dependency on the Move
  standard library. This can be done by adding an entry to the `[dependencies]`
  section of the `Move.toml`, e.g.,

  ```toml
  [dependencies]
  MoveStdlib = { local = "../../../../move-stdlib/", addr_subst = { "std" = "0x1" } }
  ```

  Note that you may need to alter the path to point to the `move-stdlib` directory under
  `<path_to_move>/language`. You can also specify git dependencies. You can read more on Move
  package dependencies [here](https://move-language.github.io/move/packages.html#movetoml).


#### Exercises
* Change the assertion to `11` so that the test fails. Find a flag that you can
  pass to the `move test` command that will show you the global state when
  the test fails. It should look something like this:
  ```
  ┌── test_mint_10 ──────
  │ error[E11001]: test failure
  │    ┌─ ./sources/FirstModule.move:24:9
  │    │
  │ 18 │     fun test_mint_10(account: signer) acquires Coin {
  │    │         ------------ In this function in 0xcafe::BasicCoin
  │    ·
  │ 24 │         assert!(borrow_global<Coin>(addr).value == 11, 0);
  │    │         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Test was not expected to abort but it aborted with 0 here
  │
  │
  │ ────── Storage state at point of failure ──────
  │ 0xc0ffee:
  │       => key 0xcafe::BasicCoin::Coin {
  │           value: 10
  │       }
  └──────────────────
  ```
* Find a flag that allows you to gather test coverage information, and
  then play around with using the `move coverage` command to look at
  coverage statistics and source coverage.

</details>

## Step 3: Designing my `BasicCoin` module<span id="Step3"><span>

In this section, we are going to design a module implementing a basic coin and balance interface, where coins can
be minted and transferred between balances held under different addresses.

The signatures of the public Move function are the following:

```
/// Publish an empty balance resource under `account`'s address. This function must be called before
/// minting or transferring to the account.
public fun publish_balance(account: &signer) { ... }

/// Mint `amount` tokens to `mint_addr`. Mint must be approved by the module owner.
public fun mint(module_owner: &signer, mint_addr: address, amount: u64) acquires Balance { ... }

/// Returns the balance of `owner`.
public fun balance_of(owner: address): u64 acquires Balance { ... }

/// Transfers `amount` of tokens from `from` to `to`.
public fun transfer(from: &signer, to: address, amount: u64) acquires Balance { ... }
```

Next we look at the data structs we need for this module.

A Move module doesn't have its own storage. Instead, Move "global storage" (what we call our
blockchain state) is indexed by addresses. Under each address there are Move modules (code) and Move resources (values).

The global storage looks roughly like this in Rust syntax:

```rust
struct GlobalStorage {
    resources: Map<address, Map<ResourceType, ResourceValue>>
    modules: Map<address, Map<ModuleName, ModuleBytecode>>
}
```

The Move resource storage under each address is a map from types to values. (An observant reader might observe that
this means each address can only have one value of each type.) This conveniently provides us a native mapping indexed
by addresses. In our `BasicCoin` module, we define the following `Balance` resource representing the number of coins
each address holds:

```
/// Struct representing the balance of each address.
struct Balance has key {
    coin: Coin // same Coin from Step 1
}
```

Roughly the Move blockchain state should look like this:

![](diagrams/move_state.png)

#### Advanced topics:
<details>
<summary><code>public(script)</code> functions</summary>

Only functions with `public(script)` visibility can be invoked directly in transactions. So if you would like to call the `transfer`
method directly from a transaction, you'll want to change its signature to:
```
public(script) fun transfer(from: signer, to: address, amount: u64) acquires Balance { ... }
```
Read more on Move function visibilities [here](https://move-language.github.io/move/functions.html#visibility).
</details>
<details>
<summary>Comparison with Ethereum/Solidity</summary>

In most Ethereum [ERC-20]((https://ethereum.org/en/developers/docs/standards/tokens/erc-20/)) contracts, the balance of each address is stored in a _state variable_ of type
<code>mapping(address => uint256)</code>. This state variable is stored in the storage of a particular smart contract.

The Ethereum blockchain state might look like this:

![](diagrams/solidity_state.png)
</details>

## Step 4: Implementing my `BasicCoin` module<span id="Step4"><span>

We have created a Move package for you in folder `step_4` called `BasicCoin`. The `sources` folder contains source code for
all your Move modules in the package, including `BasicCoin.move`. In this section, we will take a closer look at the
implementation of the methods inside [`BasicCoin.move`](./step_4/sources/BasicCoin.move).

### Compiling our code

Let's first try building the code using Move package by running the following command
in [`step_4/BasicCoin`](./step_4/BasicCoin) folder:
```bash
move build
```

### Implementation of methods
Now let's take a closer look at the implementation of the methods inside [`BasicCoin.move`](./step_4/BasicCoin/sources/BasicCoin.move).

<details>
<summary>Method <code>publish_balance</code></summary>

This method publishes a `Balance` resource to a given address. Since this resource is needed to receive coins through
minting or transferring, `publish_balance` method must be called by a user before they can receive money, including the
module owner.

This method uses a `move_to` operation to publish the resource:

```
let empty_coin = Coin { value: 0 };
move_to(account, Balance { coin:  empty_coin });
```
</details>
<details>
<summary>Method <code>mint</code></summary>

`mint` method mints coins to a given account. Here we require that `mint` must be approved
by the module owner. We enforce this using the assert statement:
```
assert!(signer::address_of(&module_owner) == MODULE_OWNER, ENOT_MODULE_OWNER);
```
Assert statements in Move can be used in this way: `assert!(<predicate>, <abort_code>);`. This means that if the `<predicate>`
is false, then abort the transaction with `<abort_code>`. Here `MODULE_OWNER` and `ENOT_MODULE_OWNER` are both constants
defined at the beginning of the module. The standard library's [`error` module] also defines common error categories we can use.
It is important to note that Move is transactional in its execution -- so
if an [abort](https://move-language.github.io/move/abort-and-assert.html) is raised no unwinding of state
needs to be performed, as no changes from that transaction will be persisted to the blockchain.

[`error` module]: https://github.com/move-language/move/blob/main/language/move-stdlib/docs/error.md

We then deposit a coin with value `amount` to the balance of `mint_addr`.
```
deposit(mint_addr, Coin { value: amount });
```
</details>

<details>
<summary>Method <code>balance_of</code></summary>

We use `borrow_global`, one of the global storage operators, to read from the global storage.
```
borrow_global<Balance>(owner).coin.value
                 |       |       \    /
        resource type  address  field names
```
</details>

<details>
<summary>Method <code>transfer</code></summary>

This function withdraws tokens from `from`'s balance and deposits the tokens into `to`s balance. We take a closer look
at `withdraw` helper function:
```
fun withdraw(addr: address, amount: u64) : Coin acquires Balance {
    let balance = balance_of(addr);
    assert!(balance >= amount, EINSUFFICIENT_BALANCE);
    let balance_ref = &mut borrow_global_mut<Balance>(addr).coin.value;
    *balance_ref = balance - amount;
    Coin { value: amount }
}
```
At the beginning of the method, we assert that the withdrawing account has enough balance. We then use `borrow_global_mut`
to get a mutable reference to the global storage, and `&mut` is used to create a [mutable reference](https://move-language.github.io/move/references.html) to a field of a
struct. We then modify the balance through this mutable reference and return a new coin with the withdrawn amount.
</details>

### Exercises
There are two `TODO`s in our module, left as exercises for the reader:
- Finish implementing the `publish_balance` method.
- Implement the `deposit` method.

The solution to this exercise can be found in [`step_4_sol`](./step_4_sol) folder.

**Bonus exercise**
- What would happen if we deposit too many tokens to a balance?

## Step 5: Adding and using unit tests with the `BasicCoin` module<span id="Step5"><span>

In this step we're going to take a look at all the different unit tests
we've written to cover the code we wrote in step 4. We're also going to
take a look at some tools we can use to help us write tests.

To get started, run the `package test` command in the [`step_5/BasicCoin`](./step_5/BasicCoin) folder

```bash
move test
```

You should see something like this:

```
INCLUDING DEPENDENCY MoveStdlib
BUILDING BasicCoin
Running Move unit tests
[ PASS    ] 0xcafe::BasicCoin::can_withdraw_amount
[ PASS    ] 0xcafe::BasicCoin::init_check_balance
[ PASS    ] 0xcafe::BasicCoin::init_non_owner
[ PASS    ] 0xcafe::BasicCoin::publish_balance_already_exists
[ PASS    ] 0xcafe::BasicCoin::publish_balance_has_zero
[ PASS    ] 0xcafe::BasicCoin::withdraw_dne
[ PASS    ] 0xcafe::BasicCoin::withdraw_too_much
Test result: OK. Total tests: 7; passed: 7; failed: 0
```

Taking a look at the tests in the
[`BasicCoin` module](./step_5/BasicCoin/sources/BasicCoin.move) we've tried
to keep each unit test to testing one particular behavior.

<details>
<summary>Exercise</summary>

After taking a look at the tests, try and write a unit test called
`balance_of_dne` in the `BasicCoin` module that tests the case where a
`Balance` resource doesn't exist under the address that `balance_of` is being
called on. It should only be a couple lines!

The solution to this exercise can be found in [`step_5_sol`](./step_5_sol)

</details>

## Step 6: Making my `BasicCoin` module generic<span id="Step6"><span>

In Move, we can use generics to define functions and structs over different input data types. Generics are a great
building block for library code. In this section, we are going to make our simple `BasicCoin` module generic so that it can
serve as a library module that can be used by other user modules.

First, we add type parameters to our data structs:
```
struct Coin<phantom CoinType> has store {
    value: u64
}

struct Balance<phantom CoinType> has key {
    coin: Coin<CoinType>
}
```

We also add type parameters to our methods in the same manner. For example, `withdraw` becomes the following:
```
fun withdraw<CoinType>(addr: address, amount: u64) : Coin<CoinType> acquires Balance {
    let balance = balance_of<CoinType>(addr);
    assert!(balance >= amount, EINSUFFICIENT_BALANCE);
    let balance_ref = &mut borrow_global_mut<Balance<CoinType>>(addr).coin.value;
    *balance_ref = balance - amount;
    Coin<CoinType> { value: amount }
}
```
Take a look at [`step_6/BasicCoin/sources/BasicCoin.move`](./step_6/BasicCoin/sources/BasicCoin.move) to see the full implementation.

At this point, readers who are familiar with Ethereum might notice that this module serves a similar purpose as
the [ERC20 token standard](https://ethereum.org/en/developers/docs/standards/tokens/erc-20/), which provides an
interface for implementing fungible tokens in smart contracts. One key advantage of using generics is the ability
to reuse code since the generic library module already provides a standard implementation and the instantiating module
can provide customizations by wrapping the standard implementation.

We provide a little module called [`MyOddCoin`](./step_6/BasicCoin/sources/MyOddCoin.move) that instantiates
the `Coin` type and customizes its transfer policy: only odd number of coins can be transferred. We also include two
[tests](./step_6/BasicCoin/sources/MyOddCoin.move) to test this behavior. You can use the commands you learned in step 2 and step 5 to run the tests.

#### Advanced topics:
<details>
<summary><code>phantom</code> type parameters</summary>

In definitions of both `Coin` and `Balance`, we declare the type parameter `CoinType`
to be phantom because `CoinType` is not used in the struct definition or is only used as a phantom type
parameter.

Read more about phantom type parameters <a href="https://move-language.github.io/move/generics.html#phantom-type-parameters">here</a>.
</details>

## Advanced steps

Before moving on to the next steps, let's make sure you have all the prover dependencies installed.

Try running `boogie /version `. If an error message shows up saying "command not found: boogie", you will have to run the
setup script and source your profile:
```bash
# run the following in move repo root directory
./scripts/dev_setup.sh -yp
source ~/.profile
```
## Step 7:  Use the Move prover<span id="Step7"><span>

Smart contracts deployed on the blockchain may manipulate high-value assets. As a technique that uses strict
mathematical methods to describe behavior and reason correctness of computer systems, formal verification
has been used in blockchains to prevent bugs in smart contracts. [
The Move prover](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/prover-guide.md)
is an evolving formal verification tool for smart contracts written in the Move language. The user can specify
functional properties of smart contracts
using the [Move Specification Language (MSL)](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md)
and then use the prover to automatically check them statically.
To illustrate how the prover is used, we have added the following code snippet to
the [BasicCoin.move](./step_7/BasicCoin/sources/BasicCoin.move):

```
    spec balance_of {
        pragma aborts_if_is_strict;
    }
```

Informally speaking, the block `spec balance_of {...}` contains the property specification of the method `balance_of`.

Let's first run the prover using the following command inside [`BasicCoin` directory](./step_7/BasicCoin/):
```bash
move prove
```

which outputs the following error information:

```
error: abort not covered by any of the `aborts_if` clauses
   ┌─ ./sources/BasicCoin.move:38:5
   │
35 │           borrow_global<Balance<CoinType>>(owner).coin.value
   │           ------------- abort happened here with execution failure
   ·
38 │ ╭     spec balance_of {
39 │ │         pragma aborts_if_is_strict;
40 │ │     }
   │ ╰─────^
   │
   =     at ./sources/BasicCoin.move:34: balance_of
   =         owner = 0x29
   =     at ./sources/BasicCoin.move:35: balance_of
   =         ABORTED

Error: exiting with verification errors
```

The prover basically tells us that we need to explicitly specify the condition under which the function `balance_of` will abort, which is caused by calling the function `borrow_global` when `owner` does not own the resource `Balance<CoinType>`. To remove this error information, we add an `aborts_if` condition as follows:

```
    spec balance_of {
        pragma aborts_if_is_strict;
        aborts_if !exists<Balance<CoinType>>(owner);
    }
```
After adding this condition, try running the `prove` command again to confirm that there are no verification errors:
```bash
move prove
```
Apart from the abort condition, we also want to define the functional properties. In Step 8, we will give more detailed introduction to the prover by specifying properties for the methods defined the `BasicCoin` module.


## Step 8: Write formal specifications for the `BasicCoin` module<span id="Step8"><span>

<details>

<summary> Method withdraw </summary>

The signature of the method `withdraw` is given below:
```
fun withdraw<CoinType>(addr: address, amount: u64) : Coin<CoinType> acquires Balance
```

The method withdraws tokens with value `amount` from the address `addr` and returns a created Coin of value `amount`.  The method `withdraw` aborts when 1) `addr` does not have the resource `Balance<CoinType>` or 2) the number of tokens in `addr` is smaller than `amount`. We can define conditions like this:

```
    spec withdraw {
        let balance = global<Balance<CoinType>>(addr).coin.value;
        aborts_if !exists<Balance<CoinType>>(addr);
        aborts_if balance < amount;
    }
```

As we can see here, a spec block can contain let bindings which introduce names for expressions. `global<T>(address): T` is a built-in function that returns the resource value at `addr`. `balance` is the number of tokens owned by `addr`. `exists<T>(address): bool` is a built-in function that returns true if the resource T exists at address. Two `aborts_if` clauses correspond to the two conditions mentioned above. In general, if a function has more than one `aborts_if` condition, those conditions are or-ed with each other. By default, if a user wants to specify aborts conditions, all possible conditions need to be listed. Otherwise, the prover will generate a verification error. However, if `pragma aborts_if_is_partial` is defined in the spec block, the combined aborts condition (the or-ed individual conditions) only *imply* that the function aborts. The reader can refer to the
[MSL](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md) document for more information.

The next step is to define functional properties, which are described in the two `ensures` clauses below. First, by using the `let post` binding, `balance_post` represents the balance of `addr` after the execution, which should be equal to `balance - amount`. Then, the return value (denoted as `result`) should be a coin with value `amount`.

```
    spec withdraw {
        let balance = global<Balance<CoinType>>(addr).coin.value;
        aborts_if !exists<Balance<CoinType>>(addr);
        aborts_if balance < amount;

        let post balance_post = global<Balance<CoinType>>(addr).coin.value;
        ensures balance_post == balance - amount;
        ensures result == Coin<CoinType> { value: amount };
    }
```
</details>

<details>
<summary> Method deposit </summary>



The signature of the method `deposit` is given below:

```
fun deposit<CoinType>(addr: address, check: Coin<CoinType>) acquires Balance
```

The method deposits the `check` into `addr`. The specification is defined below:

```
    spec deposit {
        let balance = global<Balance<CoinType>>(addr).coin.value;
        let check_value = check.value;

        aborts_if !exists<Balance<CoinType>>(addr);
        aborts_if balance + check_value > MAX_U64;

        let post balance_post = global<Balance<CoinType>>(addr).coin.value;
        ensures balance_post == balance + check_value;
    }
```

`balance` represents the number of tokens in `addr` before execution and `check_value` represents the number of tokens to be deposited. The method would abort if 1) `addr` does not have the resource `Balance<CoinType>` or 2) the sum of `balance` and `check_value` is greater than the maxium value of the type `u64`. The functional property checks that the balance is correctly updated after the execution.

</details>

<details>

<summary> Method transfer </summary>


The signature of the method `transfer` is given below:

```
public fun transfer<CoinType: drop>(from: &signer, to: address, amount: u64, _witness: CoinType) acquires Balance
```

The method transfers the `amount` of coin from the account of `from` to the address `to`. The specification is given below:

```
    spec transfer {
        let addr_from = signer::address_of(from);

        let balance_from = global<Balance<CoinType>>(addr_from).coin.value;
        let balance_to = global<Balance<CoinType>>(to).coin.value;
        let post balance_from_post = global<Balance<CoinType>>(addr_from).coin.value;
        let post balance_to_post = global<Balance<CoinType>>(to).coin.value;

        ensures balance_from_post == balance_from - amount;
        ensures balance_to_post == balance_to + amount;
    }
```

`addr_from` is the address of `from`. Then the balances of `addr_from` and `to` before and after the execution are obtained.
The `ensures` clauses specify that the `amount` number of tokens is deducted from `addr_from` and added to `to`. However, the prover will generate the error information as below:

```
error: post-condition does not hold
   ┌─ ./sources/BasicCoin.move:57:9
   │
62 │         ensures balance_from_post == balance_from - amount;
   │         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   │
   ...
```

The property is not held when `addr_from` is equal to `to`. As a result, we could add an assertion `assert!(from_addr != to)` in the method to make sure that `addr_from` is not equal to `to`.

</details>


<details>

<summary> Exercises </summary>

- Implement the `aborts_if` conditions for the `transfer` method.
- Implement the specification for the `mint` and `publish_balance` method.

The solution to this exercise can be found in [`step_8_sol`](./step_8_sol).
