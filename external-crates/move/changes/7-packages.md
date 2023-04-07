# Packages

* Status: Implemented in Move 1.5

## Introduction

Packages are a new source feature to allow Move programmers to more easily
re-use code and share it across projects.  The Move package system allows
programmers to easily:
* Define a package containing Move code;
* Parameterize a package by named addresses;
* Use a package in Move code and instantiate its named addresses;
* Build packages; and
* Work with a common interface around compiled Move artifacts.

## Motivation

Until now, when writing Move you had no easy options of being able to
package Move code into reusable chunks that could be used by others. There
was no common compiled object model or on-disk representation of compiled
Move code and its associated artifacts, e.g., source maps and generated
documentation. And finally, there was no way of easily managing named
addresses that spanned "logical groupings" of Move code.

The Move package system aims to make these actions easier by providing a
common compiled-object model and access to both compiled bytecode and
  associated compilation artifacts along with ways to:
* Declare logical packages of Move code;
* Import and use packages;
* Compile and generate associated compilation artifacts from packages; and
* Manage--expose and instantiate--named addresses in packages.

## Package Layout and Manifest Syntax

A Move package source directory contains a `Move.toml` package manifest
file along with a set of subdirectories:

```
a_move_package
├── Move.toml      (required)
├── sources        (required)
├── examples       (optional, test & dev mode)
├── scripts        (optional)
├── doc_templates  (optional)
└── tests          (optional, test mode)
```

The directories marked `required` _must_ be present in order for the directory
to be considered a Move package and to be compiled. Optional directories can
be present, and if so will be included in the compilation process. Depending on
the mode that the package is built with (`test` or `dev`), the `tests` and
`examples` directories will be included as well.

The `sources` directory can contain both Move modules and Move scripts (both
transaction scripts and modules containing script functions). The `examples`
directory can hold additional code to be used only for development and/or
tutorial purposes that will not be included when compiled outside `test` or
`dev` mode.

A `scripts` directory is supported so transaction scripts can be separated
from modules if that is desired by the package author. The `scripts`
directory will always be included for compilation if it is present.
Documentation will be built using any [documentation
templates](../move-prover/doc/user/docgen.md) present in the
`doc_templates` directory.

### Move.toml

The Move package manifest is defined within the `Move.toml` file and has the
following syntax. Optional fields are marked with `*`, `+` denotes
one or more elements:

```
[package]
name = <string>                  # e.g., "MoveStdlib"
version = "<uint>.<uint>.<uint>" # e.g., "0.1.1"
license* = <string>              # e.g., "MIT", "GPL", "Apache 2.0"
authors* = [<string>]            # e.g., ["Joe Smith (joesmith@noemail.com)", "Jane Smith (janesmith@noemail.com)"]

[addresses]  # (Optional section) Declares named addresses in this package and instantiates named addresses in the package graph
# One or more lines declaring named addresses in the following format
<addr_name> = "_" | "<hex_address>" # e.g., Std = "_" or Addr = "0xC0FFEECAFE"

[dependencies] # (Optional section) Paths to dependencies and instantiations or renamings of named addresses from each dependency
# One or more lines declaring dependencies in the following format
<string> = { local = <string>, addr_subst* = { (<string> = (<string> | "<hex_address>"))+ } }

[dev-addresses] # (Optional section) Same as [addresses] section, but only included in "dev" and "test" modes
# One or more lines declaring dev named addresses in the following format
<addr_name> = "_" | "<hex_address>" # e.g., Std = "_" or Addr = "0xC0FFEECAFE"

[dev-dependencies] # (Optional section) Same as [dependencies] section, but only included in "dev" and "test" modes
# One or more lines declaring dev dependencies in the following format
<string> = { local = <string>, addr_subst* = { (<string> = (<string> | <address>))+ } }
```

An example of the most minimal package manifest:

```
[package]
name = "AName"
version = "0.0.0"
```

An example of a more standard package manifest that also includes the Move
standard library and instantiates the named address `Std` from it with the
address value `0x1`:

```
[package]
name = "AName"
version = "0.0.0"
license = "Apache 2.0"

[addresses]
AddressToBeFilledIn = "_"
SpecifiedAddress = "0xB0B"

[dependencies]
MoveStdlib = { local = "<some_path>/move-stdlib", addr_subst = { "Std" = "0x1" } }

[dev-addresses] # For use when developing this module
AddressToBeFilledIn = "0x101010101"
```

Most of the sections in the package manifest are self explanatory, but named
addresses can be a bit difficult to understand so it's worth examining them in
a bit more detail.

## Named Addresses During Compilation

Recall that Move has [named addresses](./5-named-addresses.md) and that
named addresses cannot be declared in Move. Because of this, until now
named addresses and their values needed to be passed to the compiler on the
command line. With the Move package system this is no longer needed, and
you can declare named addresses in the package, instantiate other named
addresses in scope, and rename named addresses from other packages within
the Move package system manifest file. Let's go through each of these
individually:

## Declaration

Let's say we have a Move module in `example_pkg/sources/A.move` as follows:
```move
module NamedAddr::A {
    public fun x(): address { @NamedAddr }
}
```

We could in `example_pkg/Move.toml` declare the named address `NamedAddr` in
two different ways. The first:

```
[package]
name = "ExamplePkg"
...
[addresses]
NamedAddr = "_"
```

Declares `NamedAddr` as a named address in the package `ExamplePkg` and
that _this address can be any valid address value_. Therefore an importing
package can pick the value of the named address `NamedAddr` to be any address
it wishes. Intuitively you can think of this as parameterizing the package
`ExamplePkg` by the named address `NamedAddr`, and the package can then be
instantiated later on by an importing package.

`NamedAddr` can also be declared as:

```
[package]
name = "ExamplePkg"
...
[addresses]
NamedAddr = "0xCAFE"
```

which states that the named address `NamedAddr` is exactly `0xCAFE` and cannot be
changed. This is useful so other importing packages can use this named
address without needing to worry about the exact value assigned to it.

With these two different declaration methods, there are two ways that
information about named addresses can flow in the package graph:
* The former ("unassigned named addresses") allows named address values to flow
  from the importation site to the declaration site.
* The latter ("assigned named addresses") allows named address values to flow
  from the declaration site upwards in the package graph to usage sites.

With these two methods for flowing named address information throughout the
package graph the rules around scoping and renaming become important to
understand.

## Scoping and Renaming of Named Addresses

A named address `N` in a package `P` is in scope if:
1. It declares a named address `N`; or
2. A package in one of `P`'s transitive dependencies declares the named address
  `N` and there is a dependency path in the package graph between between `P` and the
  declaring package of `N` with no renaming of `N`.

Additionally, every named address in a package is exported. Because of this and
the above scoping rules each package can be viewed as coming with a set of
named addresses that will be brought into scope when the package is imported,
e.g., if the `ExamplePkg` package was imported, that importation would bring
into scope the `NamedAddr` named address. Because of this, if `P` imports two
packages `P1` and `P2` both of which declare a named address `N` an issue
arises in `P`: which "`N`" is meant when `N` is referred to in `P`? The one
from `P1` or `P2`? To prevent this ambiguity around which package a named
address is coming from, we enforce that the sets of scopes introduced by all
dependencies in a package are disjoint, and provide a way to _rename named
addresses_ when the package that brings them into scope is imported.

Renaming a named address when importing can be done as follows in our `P`,
`P1`, and `P2` example above:

```
[package]
name = "P"
...
[dependencies]
P1 = { local = "some_path_to_P1", addr_subst = { "P1N" = "N" } }
P2 = { local = "some_path_to_P2"  }
```

With this renaming `N` refers to the `N` from `P2` and `P1N` will refer to `N`
coming from `P1`:

```
module N::A {
    public fun x(): address { @P1N }
}
```

It is important to note that _renaming is not local_: once a named address `N`
has been renamed to `N2` in a package `P` all packages that import `P` will not
see `N` but only `N2` unless `N` is reintroduced from outside of `P`. This is
why rule (2) in the scoping rules at the start of this section specifies a
"dependency path in the package graph between between `P` and the declaring
package of `N` with no renaming of `N`."

### Instantiation

Named addresses can be instantiated multiple times across the package graph as
long as it is always with the same value. It is an error if the same named
address (regardless of renaming) is instantiated with differing values across
the package graph.

A Move package can only be compiled if all named addresses resolve to a value.
This presents issues if the package wishes to expose an uninstantiated named
address. This is what the `[dev-addresses]` section solves. This section can
set values for named addresses, but cannot introduce any named addresses.
Additionally, only the `[dev-addresses]` in the root package are included in
`dev` mode. For example a root package with the following manifest would not compile
outside of `dev` mode since `NamedAddr` would be uninstantiated:

```
[package]
name = "ExamplePkg"
...
[addresses]
NamedAddr = "_"

[dev-addresses]
NamedAddr = "0xC0FFEE"
```

## Usage, Artifacts, and Data Structures

The Move package system comes with a command line option as part of the Move
CLI `move <flags> <command> <command_flags>`. Unless a
particular path is provided, all package commands will run in the current working
directory. The full list of commands and flags for the Move CLI can be found by
running `move --help`.

### Usage

A package can be compiled either through the Move CLI commands, or as a library
command in Rust with the function `compile_package`. This will create a
`CompiledPackage` that holds the compiled bytecode along with other compilation
artifacts (source maps, documentation, ABIs) in memory. This `CompiledPackage`
can be converted to an `OnDiskPackage` and vice versa -- the latter being the data of
the `CompiledPackage` laid out in the file system in the following format:

```
a_move_package
├── Move.toml
...
└── build
    ├── <dep_pkg_name>
    │   ├── BuildInfo.yaml
    │   ├── bytecode_modules
    │   │   └── *.mv
    │   ├── source_maps
    │   │   └── *.mvsm
    │   ├── bytecode_scripts
    │   │   └── *.mv
    │   ├── abis
    │   │   ├── *.abi
    │   │   └── <module_name>/*.abi
    │   └── sources
    │       └── *.move
    ...
    └── <dep_pkg_name>
        ├── BuildInfo.yaml
        ...
        └── sources
```

See the `move-package` crate for more information on these data structures and
how to use the Move package system as a Rust library.

## Future Work

We intend on adding support for the following features over time:
* Adding a flag to print the named addresses in scope and their values for each
  of the dependencies of the package being built.
* Support renaming multiple names from dependencies to one named address, e.g.
  the following will not work today, but we'd like to support it:
    ```
    ...
    [dependencies]
    A = { local = "...", addr_subst = { "Addr" = "A" } }
    B = { local = "...", addr_subst = { "Addr" = "B" } }
    ```
* Default importation of the Move standard library: currently the Move standard
  library must be imported in the package in order to use unit testing
  features.
