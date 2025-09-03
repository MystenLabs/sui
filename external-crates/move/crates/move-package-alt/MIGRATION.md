New Move package management system
==================================

This guide describes the changes from the old package management system to the
new package management system.

The new system is currently in a prototype stage; some features are not yet
fully implemented (and these are noted in the documentation below). We've also
focused primarily on the cases where we have a valid setup, so that some of our
error messages may need some polish, and there may be cases where a
consistency check is not yet performed. Nevertheless, we hope that the
implementation is complete enough that you can successfully do an experimental
migration of real packages (and hopefully provide us concrete feedback to guide
our next steps).

Advantages of the new system
============================

Some of the benefits of the new system:

**Designed for multiple environments**
  - care has been taken to ensure that you can write a single manifest that
    works correctly for multiple environments (E.g. mainnet and testnet)

  - you can override dependencies for specific environments, so that you have
    more flexibility in your testing deployment process

  - you can add multiple environments on the same network to support more
    complex deployment scenarios; these are not strictly tied to the
    environments defined in the CLI

**Predictability and offline rebuilds**
 - dependencies get pinned to specific versions and are only updated when you
   update your manifest

 - this enables reliable builds over time - if you don't change your Move.toml,
   you don't change your dependencies

 - this also means that building doesn't need to access git once the
   dependencies are fetched for the first time

**Flexible and local naming**
 - you can rename dependencies, so that you can now depend on multiple packages
   with the same name

 - the [addresses] section is gone - dependency names and package names are
   unified

 - dependency names are not inherited from dependencies, so that understanding
   what package a name refers to only requires looking at your Move.toml and
   not your dependencies' Move.toml files


**Baseline for new features**
 - the new implementation also sets us up to implement new features (or to
   properly implement old features that don't work well), such as a stored
   upgrade cap, efficient git downloads, source verification, republishing
   dependencies on local networks, and on-chain dependencies.

Installing and running the prototype
====================================

To install the prototype, run
```sh
suiup install sui --nightly=sui-pkg-alt -y
```

This will compile `sui` from the prototype branch, so it will need the rust
toolchain installed and it will take a while to finish.

If you want to use `mvr` dependencies, you will need a patched version of `mvr`
as well. You can install it using

```sh
suiup install mvr --nightly=ml/cli-pkg-alt -y
```

Changes to the `Move.toml` file
===============================

As in the old system, packages in the new system are configured by the user
using a `Move.toml`, and the system also manages its own information in
`Move.lock` files. However, the format of these files has changed somewhat in
the new system.

### No `[addresses]` section

In the new system, you can no longer provide named addresses. Instead, the
names used for packages are taken from the names they are given in the
`[dependencies]` section (or in the `[package]` section for the package's own
name).

For example, suppose have a package called `example_package` that has a
dependency on `example_dependency`, which in turn has a dependency on
`dep_of_dep`. In your Move code, you might have

```move
module example_package::m;
use example_dependency::n;
use dep_of_dep::p;
```

In the old style, the `Move.toml` for `example_package` might look as follows:

```toml
[package]
name = "Example"

[dependencies]
ExampleDep = { local = "../example_depenency" }

[addresses]
example_package = "0x0"
```

The names `ExampleDep` and `Example` are (mostly) ignored; the source code name
`example_package` would come from the `addresses` section; the name
`example_dependency` would come from the `../example_dependency/Move.toml`, and
the name `dep_of_dep` would come from the `Move.toml` file of whatever package
`example_dependency` depends on.

In the new system, the `Move.toml` for `example_package` would look like this
(see below for more details on the `[dependencies]` section):

```toml
[package]
name = "example_package"

[dependencies]
example_dependency = { local = "../example_dependency" }
dep_of_dep = { git = "..." }
```

Here, you must define a dependency for every package that you name in your
source code (although you don't need to add a dependency if you don't directly
use the package). The name you use in `Move.toml` for the dependency is what
defines the name in your Move code (and the name you use in the `[package]`
section is how you refer to your own address in Move).

### The `[package]` section

There are two small changes to the `[package]` section of the manifest. The
first is that you should change the `name` used in your manifest to match the
name used in your code.

The second is the new `implicit-dependencies = false` field. In the old system,
adding an explicit system dependency disabled implicit dependencies. In the new
system, if you want to turn off implicit dependencies you must explicitly say
`implicit-dependencies = false` in the `[package]` section.

However, we advise against disabling implicit dependencies.

### The `[dependencies]` section

In the `[dependencies]` section, you must define each package that your source
code refers to by name. As menioned above, the name you give the dependency is
the same as the name used in your source code. So if you want to include a
dependency on `../example` and refer to it as `example_dependency` you would write:

```toml
[dependencies]
example_dependency = { local = "../example" }
```

As in the previous system you can have `git`, `local`, or `r.<resolver>`
dependencies (e.g. `r.mvr`). We are planning to add support for `on-chain`
dependencies as well, but this is not yet implemented.

In addition to the type-specific fields (like `rev` for `git` dependencies, all
dependencies allow an `override = true`; this works exactly the same way that
it did in the old system.

We encourage consistency by requiring that the name that you give a dependency
(`example_dependency` in the example) must match the name that the dependency
gives itself (the `name` field in `../example/Move.toml`). However, if you want
to override the name, you can add a `rename-from` field to the dependency. For
example, if `../example/Move.toml` had `name = "example"`, you could write

```toml
[dependencies]
example_dependency = { local = "../example", rename-from = "example" }
```

Then your Move code would refer to the package using `example_dependency` even
though the Move code for the dependency will refer to itself using `example`.

This allows you to easily mix dependencies that happen to have the same name:

```toml
[dependencies]
alices_mathlib = { git = "https://github.com/alice.git", rename-from = "mathlib" }
bobs_mathlib = { git = "https://github.com/bob.git", rename-from = "mathlib" }
```

#### Note on git dependencies with short shas

The new system handles git dependencies much more efficiently than the old system. First of all,
once your dependencies been pinned and fetched, you no longer need to interact
with git on subsequent builds.

Secondly, the new system only downloads the files and versions that you
require, so the downloads are much faster.

However, if you have a git dependency with `rev = "<commit hash>"` and `<commit
hash>` is not a full 40-character SHA, we are forced to download the entire
commit history to expand the SHA; if you want to add a dependency on a
particular commit, you should include the entire 40-character SHA.


### The `[environments]` and `[dep-replacements]` sections

In the new system, all package operations happen in the context of an
"environment". Every environment has a name and a chain ID. There are two
implicitly defined environments: `mainnet` and `testnet`. In addition, you can
add your own environments in the `[environments]` section:

```toml
[environments]
testnet_alpha = "4c78adac"
```

You can replace your dependencies in specific environments by adding a
`[dep-replacements]` section:

```toml
[dependencies]
example_dependency = { git = "example.git" }

[dep-replacements]
testnet.example_dependency = { git = "example.git", rev = "testnet-1.2.3" }
```

 - Note: TOML also allows you to add `example_dependency = {...}` to
   `[dep-replacements.testnet]`; this is exactly the same as adding
   `testnet.example_dependency = {...}` to `[dep-replacements]`)

#### Environment-specific dependency fields

In addition to the normal fields you can write in the `[dependencies]` section
(e.g. `local`, `override`, `rename-from`, etc), there are additional fields
that you can add to dependencies in the `[dep-replacements]` section:

 - You can add a `use-environment` field to indicate which of the dependency's
   environments to use. For example, if the dependency defines a
   `testnet_alpha` environment, you can depend on that environment as follows:

   ```toml
   [dependencies]
   example_dependency = { git = "example.git" }

   [dep-replacements]
   testnet.example_dependency = { use-environment = "testnet_alpha" }
   ```

   This will use the published address for the `testnet_alpha` version of
   `example_dependency` when publishing this package to `testnet`.

 - Normally you don't need to specify dependencies' addresses in your manifest,
   but if you depend on legacy packages or packages that have been published
   but haven't updated their lockfiles, you can override the `published-at` and
   `original-id` fields for the dependency (if you do one of these you must do
   both):

   ```toml
   [dep-replacements]
   testnet.example_dependency = { published-at = "0x5678", original-id = "0x1234" }
   ```

 - Note: the `git` fields are currently not merged between `[dependencies]` and
   `[dep-replacements]`; for example you can't just give a different `rev`
   field in the `[dep-replacements]`, you must also give the `git` field.
   Moreover, if you do give a `git` field, the `subdir` and `rev` fields are
   not copied over. We are considering changing this behavior


CLI changes
===========

Our prototype is integrated into the `sui` binary, so most of the commands you
are familiar with work the same way. There are a few changes though:

### Compiling for specific environments

Because of `dep-replacements` (and also some details about how package
conflicts are determined), all operations must be done in a specific
environment. By default, the `sui` binary tries to choose the right environment
based on the sui client environment (as determined by `sui client active-env`).

If there is not an obvious choice of environment (either because the manifest
declares multiple environments with the same chain ID or because it doesn't
declare one at all[^1]), you will have to choose a specific environment by
passing `-e <environment>`.

[^1]: In the current prototype, that means that to publish to devnet or
    localnet you will need to have environments defined for them. We have a
    `[dev-environments]` feature for this purpose, but the implementation of
    that feature is not currently complete.
