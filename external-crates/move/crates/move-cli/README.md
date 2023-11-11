# Move CLI

The Move command-line interface (Move CLI) is a tool that provides an easy way to interact with Move, to experiment
with writing and running Move code, and to experiment with developing new tools useful
for Move development. To reflect this, the Move CLI commands are grouped into
three main subcommands:
* **package commands**: are commands to create, compile, and test Move packages, as well as perform other operations related to packages. These do not rely on a Move Adapter implementation nor an implementation of storage.
* **sandbox commands**: are commands that allow you to write Move modules and scripts, write and run scripts and tests, and view the resulting state of execution in a local sandboxed environment.
* **experimental commands**: are experimental commands that are currently in development.

Every Move CLI command, with the exception of `package create`, is expected to be run within the context of a [Move package](https://move-language.github.io/move/packages.html).

## Installation
```shell
$ cargo install --path move/language/tools/move-cli
```
or
```shell
$ cargo install --git https://github.com/move-language/move move-cli --branch main
```

This will install the `move` binary in your Cargo binary directory. On
macOS and Linux this is usually `~/.cargo/bin`. You'll want to make sure
this location is in your `PATH` environment variable.

Now you should be able to run the Move CLI:

```shell
$ move
Move 0.1.0
CLI frontend for Move compiler and VM

USAGE:
    move [FLAGS] [OPTIONS] <SUBCOMMAND>
  ...
```

We'll go through the most common Move CLI commands and flags here, however
you can find the complete list of commands available by calling `move
--help`.  Additionally, the complete list of flags and options available
for each Move CLI command can be found by passing the `--help` flag to it,
i.e., `move <command> --help`.

## Package Commands

Package commands provide wrappers with sane defaults around other commands
that are provided either by various Move tools, compiler, or prover.

The `move new` command will create a new empty Move package:
```shell
$ move new <package_name> # Create a Move package <package_name> under the current dir
$ move new <package_name> -p <path> # Create a Move package <package_name> under path <path>
```

From within a package's root directory, you can build the modules and/or scripts that you have written in the package with:
```shell
$ move build # Builds the Move package you are currently in
$ move build -p <path> # Builds the Move package at <path>
```

The compiled artifacts will by default be stored in the `build` directory. You
can change where the build artifacts are saved by passing the optional `--build-dir` flag:

```shell
$ move build --build-dir <path_to_save_to> # Build current Move package and save artifacts under <path_to_save_to>
```

You can verify the specifications in a Move package using the Move Prover with the `prove` command:

```shell
$ move prove # Verify the specifications in the current package
$ move prove -p <path> # Verify the specifications in the package at <path>
```

In order to run the Move Prover [additional tools need to be
installed](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/install.md).
Information on the Move Prover and its configuration options can be found
[here](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/prover-guide.md)
and
[here](https://github.com/move-language/move/blob/main/language/move-prover/doc/user/spec-lang.md).

You can also run unit tests in a package using the `test` command

```shell
$ move test # Run Move unit tests in the current package
$ move test -p <path> # Run Move unit tests in the package at <path>
```
## Sandbox Commands

The sandbox allows you to experiment with writing and running Move code without
validators, a blockchain, or transactions. Persistent data is stored on-disk in
a directory structure that mimics the Move memory model

### Project structure

Each sandbox command is run in the context of a Move package. So let's create a
Move package that we'll use for the code in this README and `cd` into it:

```shell
$ move new readme
$ cd readme
```

### Compiling and running scripts

Let's first start out with a simple script that prints its [`signer`](https://move-language.github.io/move/signer.html).
Create a file named `sources/debug_script.move` and type the following into it:

```rust
// sources/debug_script.move
script {
use std::debug;
fun debug_script(account: signer) {
    debug::print(&account)
}
}
```

Before we can run this however, we need to import the Move standard library
nursery in order to have access to the `Debug` module and `Std` [named
address](https://move-language.github.io/move/address.html#named-addresses).
You can specify dependencies locally, or using a Git URL. Here, we will specify
it using Git, so add the following to the `Move.toml` file in the `readme`
directory:

```toml
[addresses]
std = "0x1" # Specify and assign 0x1 to the named address "std"

[dependencies]
MoveNursery = { git = "https://github.com/move-language/move.git", subdir = "language/move-stdlib/nursery", rev = "main" }
#                ^                    ^                              ^                                       ^
#            Git dependency       Git clone URL       Subdir under git repo (optional)           Git revision to use
```

Now let's try running the script -- the very first time may take some time since the package command will clone the repository at the given URL, but subsequent calls should be fast:

```shell
$ move sandbox run sources/debug_script.move --signers 0xf
[debug] (&) { 0000000000000000000000000000000f }
```

The `--signers 0xf` argument indicates which account address(es) have signed
off on the script. Omitting `--signers` or passing multiple signers to this
single-`signer` script will trigger a type error.

### Passing arguments

The CLI supports passing non-`signer` arguments to `move sandbox run` via `--args`. The following argument types are supported:
* `bool` literals (`true`, `false`)
* `u64` literals (e.g., `10`, `58`)
* `address` literals (e.g., `0x12`, `0x0000000000000000000000000000000f`)
* hexadecimal strings (e.g., `'x"0012"'` will parse as the `vector<u8>` value `[00, 12]`)
* ASCII strings (e.g., `'b"hi"'` will parse as the `vector<u8>` value `[68, 69]`)

### Publishing new modules

When executing a transaction script you'll often want to call into different
Move modules, like in the example above with the `Debug` module. New modules can
be added to the `sources` directory in the package where the CLI is being
invoked. You can also add dependencies on other packages to have access to the
modules that they define (just like we did with the `Debug` module above). The
`move sandbox run` command will compile and publish each module in the package, and
in each of the package's transitive dependencies, before running the given script.

Try saving this code in `sources/Test.move`:

```rust
// sources/Test.move
module 0x2::Test {
    use std::signer;

    struct Resource has key { i: u64 }

    public fun publish(account: &signer) {
        move_to(account, Resource { i: 10 })
    }

    public fun write(account: &signer, i: u64) acquires Resource {
        borrow_global_mut<Resource>(signer::address_of(account)).i = i;
    }

    public fun unpublish(account: &signer) acquires Resource {
        let Resource { i: _ } = move_from(signer::address_of(account));
  }
}
```

Now, try

```shell
$ move build
```

This will cause the CLI to compile and typecheck the modules under
`sources`, but it won't publish the module bytecode under `storage`. You can
compile and publish the module by running the `move sandbox publish` command
(here we pass the `-v` or verbose flag to get a better understanding of what's
happening):

```shell
$ move sandbox publish -v
Found 1 modules
Publishing a new module 00000000000000000000000000000002::Test (wrote 253 bytes)
Wrote 253 bytes of module ID's and code
```

Now, if we take a look under `storage`, we will see the published bytecode
for our `Test` module:

```shell
$ ls storage/0x00000000000000000000000000000002/modules
Test.mv
```

We can also inspect the compiled bytecode in storage using `move sandbox view`:

```shell
$ move sandbox view storage/0x00000000000000000000000000000002/modules/Test.mv
module 2.Test {
struct Resource has key {
  i: u64
}

public publish() {
  0: MoveLoc[0](Arg0: &signer)
  1: LdU64(10)
  2: Pack[0](Resource)
  3: MoveTo[0](Resource)
  4: Ret
}
public unpublish() {
  0: MoveLoc[0](Arg0: &signer)
  1: Call[3](address_of(&signer): address)
  2: MoveFrom[0](Resource)
  3: Unpack[0](Resource)
  4: Pop
  5: Ret
}
public write() {
  0: CopyLoc[1](Arg1: u64)
  1: MoveLoc[0](Arg0: &signer)
  2: Call[3](address_of(&signer): address)
  3: MutBorrowGlobal[0](Resource)
  4: MutBorrowField[0](Resource.i: u64)
  5: WriteRef
  6: Ret
}
}
```

You can also look at the compiled bytecode before publishing to `storage` by
running either `move disassemble --name <module_name>` or `move
disassemble --name <module_name> --interactive` to interactively inspect the
bytecode and how it relates to the Move source code:

```shell
$ move disassemble --name Test --interactive # You can quit by pressing "q"
$ move disassemble --name Test
// Move bytecode v4
module 2.Test {
struct Resource has key {
        i: u64
}

public publish() {
B0:
        0: MoveLoc[0](account: &signer)
        1: LdU64(10)
        2: Pack[0](Resource)
        3: MoveTo[0](Resource)
        4: Ret
}
public unpublish() {
B0:
        0: MoveLoc[0](account: &signer)
        1: Call[3](address_of(&signer): address)
        2: MoveFrom[0](Resource)
        3: Unpack[0](Resource)
        4: Pop
        5: Ret
}
public write() {
B0:
        0: CopyLoc[1](i: u64)
        1: MoveLoc[0](account: &signer)
        2: Call[3](address_of(&signer): address)
        3: MutBorrowGlobal[0](Resource)
        4: MutBorrowField[0](Resource.i: u64)
        5: WriteRef
        6: Ret
}
}
```

### Updating state

Let's exercise our new `Test` module by running the following script:

```rust
// sources/test_script.move
script {
use 0x2::Test;
fun test_script(account: signer) {
    Test::publish(&account)
}
}
```

This script invokes the `publish` function of our `Test` module, which will
publish a resource of type `Test::Resource` under the signer's account.
Let's first see what this script will change without committing those
changes first. We can do this by passing the `--dry-run` flag:

```shell
$ move sandbox run sources/test_script.move --signers 0xf -v --dry-run
Compiling transaction script...
Changed resource(s) under 1 address(es):
  Changed 1 resource(s) under address 0000000000000000000000000000000f:
    Added type 0x2::Test::Resource: [10, 0, 0, 0, 0, 0, 0, 0] (wrote 40 bytes)
Wrote 40 bytes of resource ID's and data
      key 0x2::Test::Resource {
           i: 10
      }
Discarding changes; re-run without --dry-run if you would like to keep them.
```

Everything looks good, so we can run this again, but this time commit the
changes by removing the `--dry-run` flag:

```shell
$ move sandbox run sources/test_script.move --signers 0xf -v
Compiling transaction script...
Changed resource(s) under 1 address(es):
  Changed 1 resource(s) under address 0000000000000000000000000000000f:
    Added type 0x2::Test::Resource: [10, 0, 0, 0, 0, 0, 0, 0] (wrote 40 bytes)
Wrote 40 bytes of resource ID's and data
      key 0x2::Test::Resource {
            i: 10
      }
```

While the verbose flag used above (`-v`) shows resource changes, it is also
possible to view them manually.
We can inspect the newly published resource using `move sandbox view` since
the change has been committed:

```shell
$ move sandbox view storage/0x0000000000000000000000000000000f/resources/0x00000000000000000000000000000002::Test::Resource.bcs
key 0x2::Test::Resource {
    i: 10
}
```

#### Cleaning state

Since state persists from one call to the Move CLI to another, there will
frequently be times where you want to start again at a clean state.  This
can be done using the `move sandbox clean` command which will remove the
`storage` and `build` directories:

```shell
$ move sandbox view storage/0x0000000000000000000000000000000f/resources/0x00000000000000000000000000000002::Test::Resource.bcs
resource 0x2::Test::Resource {
        i: 10
}
$ move sandbox clean
$ move sandbox view storage/0x0000000000000000000000000000000f/resources/0x00000000000000000000000000000002::Test::Resource.bcs
Error: `move sandbox view <file>` must point to a valid file under storage
```

### Expected Value Testing with the Move CLI

As mentioned previously, Move has a unit testing framework. However, unit tests
cannot test everything -- in particular testing for events cannot be easily
done. To help with writing tests that need to check for events, and expect
specific states, the Move CLI also has a built-in expected-value testing
framework. Each test is run independently in its own sandbox so state does not
persist from one test to another.

Each test is structured as a Move package along with an additional `args.txt` file that
specifies a sequence of Move CLI commands that should be run in that
directory.
Additionally, there must be an `args.exp` file that contain the expected
output from running the sequence of Move CLI commands specified in the
`args.txt` file for that test.

For example, if we wanted to create a Move CLI test that reran all of the
commands that we've seen so far, we could do so by adding an `args.txt`
to the `readme` directory that we created at the start and that we've been
adding scripts and modules to:

```
readme/
├── args.txt
├── Move.toml
└── sources
    ├── debug_script.move
    ├── Test.move
    └── test_script.move
```

And, where the `args.txt` file contains the following Move CLI commands:

```shell
$ cd ..
$ cat readme/args.txt
## Arg files can have comments!
sandbox run sources/debug_script.move --signers 0xf
sandbox run sources/debug_script.move --signers 0xf
build
sandbox publish
sandbox view storage/0x00000000000000000000000000000002/modules/Test.mv
sandbox run sources/test_script.move --signers 0xf -v
sandbox view storage/0x0000000000000000000000000000000f/resources/0x00000000000000000000000000000002::Test::Resource.bcs
```

We can then use the `move sandbox test` command and point it at the `readme` directory to run each of these
Move CLI commands for us in sequence:

```shell
$ move sandbox exp-test -p readme
...<snipped output>
0 / 1 test(s) passed.
Error: 1 / 1 test(s) failed.
```

However, as we see this test will fail since there is no `args.exp` file for the test
yet. We can generate this expectation file by setting the `UPDATE_BASELINE`
environment variable when running the test:

```shell
$ UPDATE_BASELINE=1 move sandbox exp-test -p readme
1 / 1 test(s) passed.
```

There should now be an `args.exp` file under the `readme` directory that
contains the expected output of running the sequence of Move CLI commands
in the `args.txt` file:

```shell
$ cat readme/args.exp
Command `sandbox run sources/debug_script.move --signers 0xf`:
[debug] (&) { 0000000000000000000000000000000f }
Command `sandbox run sources/debug_script.move --signers 0xf --mode bare`:
...
```

#### Testing with code coverage tracking

Code coverage has been an important metric in software testing. In Move CLI expected value tests, we
address the need for code coverage information with an additional flag,
`--track-cov`, that can be passed to the `move sandbox exp-test` command.

Note: To view coverage information, the Move CLI must be installed with the `--debug` flag;
i.e., `cargo install --debug --path move/language/tools/move-cli`.

Using our running example to illustrate:
```shell
$ move sandbox exp-test -p readme --track-cov
1 / 1 test(s) passed.
Module 00000000000000000000000000000002::Test
        fun publish
                total: 5
                covered: 5
                % coverage: 100.00
        fun unpublish
                total: 6
                covered: 0
                % coverage: 0.00
        fun write
                total: 7
                covered: 0
                % coverage: 0.00
>>> % Module coverage: 27.78
```

The output indicates that not only the test is passed, but also that 100%
instruction coverage is observed in the `publish` funciton. This is expected
as the whole purpose of our `test_script.move` is to run the `publish` function.
At the same time, the other two functions, `unpublish` and `write`, are never
executed, making the average coverage 27.78% for the whole `Test` module.

Internally, Move CLI uses the tracing feature provided by the Move VM to record
which instructions in the compiled bytecode are executed and uses this
information to calculate code coverage. Instruction coverage in Move can
usually serve the purpose of line coverage in common C/C++/Rust coverage
tracking tools.

Note that the coverage information is aggregated across multiple `run` commands
in `args.txt`. To illustrate this, suppose that we have another test script,
`test_unpublish_script.move`, under `readme/sources` with the following
content:

```rust
script {
use 0x2::Test;
fun test_unpublish_script(account: signer) {
    Test::unpublish(&account)
}
}
```

We further add a new command to the end of `args.txt`
(`args.exp` needs to be updated too).
```shell
sandbox run sources/test_unpublish_script.move --signers 0xf -v
```

Now we can re-test the `readme` again
```shell
$ move sandbox exp-test -p readme --track-cov
1 / 1 test(s) passed.
Module 00000000000000000000000000000002::Test
        fun publish
                total: 5
                covered: 5
                % coverage: 100.00
        fun unpublish
                total: 6
                covered: 6
                % coverage: 100.00
        fun write
                total: 7
                covered: 0
                % coverage: 0.00
>>> % Module coverage: 61.11
```

This time, note that the `unpublish` function is 100% covered too and the
overall module coverage is boosted to 61.11%.

### Detecting breaking changes

The `move sandbox publish` command automatically detects when upgrading a module may lead to a breaking change.
There are two kinds of breaking changes:

* Linking compatibility (e.g., removing or changing the signature of a public function that is invoked by other modules, removing a
struct or resource type used by other modules)
* Layout compatibility (e.g., adding/removing a resource or struct field)

The breaking changes analysis performed by `move sandbox publish` is necessarily conservative. For example, say we `move sandbox publish` the following
module:

```
address 0x2 {
module M {
    struct S has key { f: u64, g: u64 }
}
}
```

and then wish to upgrade it to the following:

```
address 0x2 {
module M {
    struct S has key { f: u64 }
}
}
```

Running `move sandbox publish` on this new version will fail:

```
Breaking change detected--publishing aborted. Re-run with --ignore-breaking-changes to publish anyway.
Error: Layout API for structs of module 00000000000000000000000000000002::M has changed. Need to do a data migration of published structs
```

In this case, we know we have not published any instances of `S` in global storage, so it is safe to re-run `move sandbox publish --ignore-breaking-changes` (as recommended).
We can double-check that this was not a breaking change by running `move sandbox doctor`.
This handy command runs exhaustive sanity checks on global storage to detect any breaking changes that occurred in the past:
* All modules pass the bytecode verifier
* All modules link against their dependencies
* All resources deserialize according to their declared types
* All events deserialize according to their declared types
