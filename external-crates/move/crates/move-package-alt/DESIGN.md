# Package Management Alt Design

In this proposal, the `Move.lock` file contains all the information about the published versions of
packages. It is updated when packages are published or upgraded. It should contain enough
information to consistently rebuild a package.

This document is the current version of the design in [notion][notion-design]; See also
[overview][notion-overview] for a higher level motivation and outline, and [user
stories][notion-userstories] for a walkthrough of usage scenarios.

[notion-design]: https://www.notion.so/mystenlabs/Package-Management-Revamp-Concrete-Design-1b56d9dcb4e980ccb06ad12ad18004c0?pvs=4
[notion-overview]: https://www.notion.so/Package-management-revamp-overview-1aa6d9dcb4e980128c1bc13063c418c7?pvs=21
[notion-userstories]: https://www.notion.so/Package-management-user-stories-1bd6d9dcb4e98005a4a7ddea4424f757?pvs=21

# TODOs

 - careful rollout plan - parallel with extisting system?
 - sui move build/test now will use the active env from cli to match which dependencies to use
 - two packages share an identity if they have the same original-id in any environment
 - maybe need to do additional identity checks when running `--publish-unpublished-deps`
 - cached deps are read-write but you can’t use dirty ones without asking
 - non-git local deps are a loud warning

# Points of contention and remaining questions

(*) indicates @Michael George ‘s preference

 - For `move build` and `move test`: should we build/test for all environments or only for one?
     - * opt 1: Build/test for all environments by default
         - Con: efficiency, but compilation is fast
         - Con: Need to do work to deduplicate warnings from different environments
     - opt 2: Build/test for only current environment by default
         - Con: defers error detection
         - Con: can be easy to forget to test with a specific configuration
         - Con: more complicated CI (where presumably you do actually want to test everything)
     - **Resolution**: it will use the corresponding `[dep-replacements]` in `build` and `test` as per
       the active environment from the cli, unless you pass `--env`. We also allow `--all-envs` for `test`,
       which runs tests in all environments.
 - For cached deps in `~/.move`: read only?
     - opt 1: read-only
         - Cons: can’t easily fiddle with them during debugging. Except (1) we
           don’t support that anyway, and (2) any decent editor will just say
           “file is read only, are you sure you want to write it?” and change it
           to r/w for you
     - opt 2: read-write
         - Cons: if a user accidentally changes it during a debugging session
           (assuming we don’t continue to duplicate dependency sources in each
           package), it will change for everyone. Except we could probably just
           do a git clean before we use a cached dependency (we should probably
           do this anyway)
     - Resolution: read-write but require opt-in to build with dirty cache (with
       an `--allow-dirty-cache` opt-in)
 - For local deps that don’t share a parent git repository: error or warning?
     - **Resolution:** loud, clear warning that explains why this is bad
 - Include the dependencies in the publication records in Move.lock?
     - **Resolution:** omit them.
     - The requirement from the publisher for successful source verification is that you have a
       single revision with the "correct" source and a lockfile containing the published address
     - Note that "correct" means that the compiled bytecode is the same - there can still be
       changes to macros, comments, etc! We might want to make this more secure in the future, e.g.
       by including some kind of digest or publishing macros on-chain

 - Question: Consider you have pkg A that depends on pkg B, which depends on pkg C.
        Today, pkg A can directly use pkg C's code due to global inherited addresses.
        In the new design, should it be allowed, and if yes, how -- which name should A
        use to refer to the package and its modules?
        We should also consider how this would work in the case of bytecode dependencies.

 - Question: What should we name the new edition that contains these changes?
     - Move 2025!

 - Question: What does the `flavor` field currently do? How does it interact with `edition`? Do we
   need to support it?
 - Question: What is `--dependencies-are-root`? Do we still need it?
 - Question: Do we want a `build` section in the manifest?
 - Question: `override = true` for transitive deps
     - If A depends on B and C, B depends on Dv1, C depends on Dv2, Dv1 depends on Ev1, and Dv2 depends on Ev2, does A need to specify an override for E?
     - If so, should we change this?
 - Worry: packages are now identified by published address instead of name. Published addresses are
   chain-specific, which is fine; we can detect errors separately on different chains. However, what
   about ephemeral networks and `--publish-unpublished-deps`? Is it ok that we just publish two
   different versions of a package and treat them as different packages (you would already have
   gotten a warning if they are published on one of the networks you care about).

 - Question: how do we help users who are publishing using mechanisms other than the
   CLI to keep their lock files up to date?
     - When we "publish" with --dump, we not only output the bytecode, but an additional file
       containing the information needed to update the lockfile; we then have a separate command
       that users can use to update the lock file after publishing

# Example manifest and lock files

Move.toml:

```toml
[package]
name = "example"
edition = "2025"
...

[environments]
mainnet = "35834a8a"
testnet = "4c78adac"

[dependencies]
foo = {
	rename-from = "Foo", # needed for name consistency - see Validation section
	override = true,     # same as today
	git = "https://.../foo.git", # resolver specific fields
	rev = "releases/v4",
}

non = {
	rename-from = "Foo", # needed for name consistency - see Validation section
	git = "https://.../non.git", # resolver specific fields
	rev = "releases/v1",
}

bar = { r.mvr = "@protocol/bar" }

[dep-replacements]
# used to replace dependencies for specific environments
mainnet.foo = {
	git = "...",          # override the source of the dep
	original-id = "....", # add an explicit address; see Backwards Compatibility
	published-at = "...",
	use-environment = "mainnet_alpha" # override/specify the dep's environment
}
```

Move.lock (contains `unpublished` and entries for environments defined in `Move.toml`):

```toml
[move]
version = 4

# The `pinned` section contains a dependency graph for each environment. Each node in
# the graph contains the pinned dependency location, a digest of the manifest of the dependency (to
# detect local changes), and a set of outgoing edges. The edges are labeled by the name of the dependency.
#
# The identities of the nodes are arbitrary, but it seems nice to generate them from the package
# names that dependencies declare for themselves (adding numbers to disambiguate if there are 
# collisions).
# There is also a node for the current package; the only thing that makes it special is that it has
# the name of the current package as its identity (it will also always have `{local = "."}` as its
# source)
[pinned.mainnet.example]
source = { root = true }
manifest_digest = "..."

deps.std = "MoveStdlib"
deps.sui = "Sui"
deps.foo = "Foo_0" # ensure rename-from is respected
deps.non = "Foo_1" # ensure rename-from is respected
deps.bar = "bar"

[pinned.mainnet.MoveStdlib]
source = { git = "...", path = "...", rev = "1234" }
manifest_digest = "..."
deps = {}

[pinned.mainnet.Sui]
source = { git = "...", path = "...", rev = "1234" }
manifest_digest = "..."
deps.std = "MoveStdlib"

[pinned.mainnet.Foo_0]
source = { git = "...", path = "...", rev = "bade" }
manifest_digest = "..."
deps.std = "MoveStdlib"
deps.sui = "Sui"

[pinned.mainnet.Foo_1]
source = { git = "...", path = "...", rev = "baaa" }
manifest_digest = "..."
deps.std = "MoveStdlib"
deps.sui = "Sui"

[pinned.mainnet.bar]
source = { git = "...", path = "...", rev = "bara" }
manifest_digest = "..."
deps.baz = "baz"
deps.std = "MoveStdlib"
deps.sui = "Sui"

[pinned.mainnet.baz]
source = { git = "...", path = "...", rev = "baza" }
manifest_digest = "..."
deps.std = "MoveStdlib"
deps.sui = "Sui"

[pinned.mainnet.[...]] # other transitive dependencies from example

# the same for testnet environment as above
[pinned.testnet.MoveStdlib]
source = { git = "...", path = "...", rev = "1234" }
manifest_digest = "..."
deps = {}

[pinned.testnet.Sui]
source = { git = "...", path = "...", rev = "1234" }
manifest_digest = "..."
deps.std = "MoveStdlib"

# the same for other defined environments
[pinned.env.Sui]
source = { git = "...", path = "...", rev = "1234" }
manifest_digest = "..."
deps.std = "MoveStdlib"
deps.sui = "Sui"

# The `published` section contains a record for the current versions published on each declared
# environment (if any).
[published.mainnet] # metadata from most recent publish to mainnet
chain-id = "35834a8a"
published-at = "..."
original-id  = "..."
upgrade-cap = "..."

build-config = "..."
toolchain-version = "..."
version = "3"

[published.testnet] # metadata from most recent publish to testnet
chain-id = "4c78adac"
published-at = "..."
original-id = "..."
upgrade-cap = "..."
toolchain-version = "..."
build-config = "..."
version = "5"
```

`.Move.<environment>.lock` (contains information for a chain not included in `Move.toml`, should be
gitignored and hidden - think `.Move.localnet.lock`):

```toml
chain-id = "840cd942"
published-at = "..."
original-id = "..."
upgrade-cap = "..."
toolchain-version = "..."
build-config = "..."
version = "0"
```

# Schema for manifest and lock files

```
Move.toml
    package
        name : PackageName
        edition : "2025"
        version : Optional String
        license : Optional String
        authors : Optional Array of String
        implicit_dependencies : Boolean (default true)

    environments : EnvironmentName → ChainID

    dependencies : PackageName → (SourceDependencyInfo + DependencyLocation)

    dep-replacements : EnvironmentName → PackageName → (Optional DependencySpec + Optional AddressInfo)

Move.lock
    move
        version : 4

    pinned : EnvironmentName -> PackageID →
        source : PinnedDependencyLoc
        manifest_digest : Digest
        deps : PackageName → PackageID

    published : EnvironmentName → PublishedMetadata

Move.<EnvironmentName>.lock
    PublishedMetadata

# TODO - check if this is correct and fix
DependencyLocation: # information used to locate a dependency
    source: ResolverName
    additional resolver-dependent fields

PinnedDependencyLoc:
    DependencyLocation with additional constraints - see Pinning

SourceDependencyInfo: # additional properties of a dependency
    override : Optional Boolean
    rename-from : Optional PackageName
    use-environment : Optional EnvironmentName

AddressInfo:
    published-at : ObjectID
    original-id  : ObjectID

PublishedMetadata: # snapshot of an on-chain published version
    # Note: we will always output the optional fields, but we may not be able to
    # when migrating historical packages.
    chain-id : ChainID
    upgrade-cap : Optional ObjectID
    + AddressInfo
    version : String # used to report dependency changes during publication

    toolchain-version : Optional ToolchainVersion
    build-config : Optional OpaqueBuildConfig
```

# Internal Operations

## Pinned dependencies

The purpose of the lockfile is to give stability to developers by pinning consistent (transitive)
dependency versions. These pinned versions are then used by other developers working on the same
project, by CI, by source verifiers, and so forth.

A pinned dependency is one that guarantees that future lookups of the same dependency
will yield the same source package. For example, a git dependency with a branch revision is not
pinned, because the branch may be moved to a different commit, which might have different source
code. To pin this kind of dependency, we must replace the branch name with a commit hash -
downloading the same commit should always produce the same source code.

Although we expect pinned dependencies to not change, there are a few situations where they could.
One possiblity is local dependencies - if a single repository contains both a dependency and the
depending project, we would want changes in the dependency to be reflected immediately in the
depending project. Moreover, this doesn't cause inconsistencies in CI because presumably those
changes would be committed together. Another example would be if a user wants to temporarily change
a cached dependency during debugging to rerun a test or something. To handle these situations, we
will store digests of all transitive dependency manifests and repin if any of them change.

Dependencies are always pinned as a group and are only repinned in two situations:

1. The user explicitly asks for it by running `sui move update-deps`. This command will repin all
   dependencies.
2. If the manifest has changed, then all dependencies are repinned.

Note: we had considered only repinning the dependencies that had changed and allowing the user to
repin only specific deps, but this leads to a lot of confusing corner cases. This is consistent with
the philosophy is that pinning is a mechanism for reliable rebuilding and not for dependency version
management - if a user cares about the exact version of a dependency then they can indicate that by
referring to that version in their manifest.

## Environment-Specific Dependency Replacements

In addition to the main set of dependencies, the user can also override dependencies in specific
environments using a `[dep-replacements]` section. These have a slightly different set of
information - for example they can include the on-chain addresses for packages (although this should
only be used in the case of a legacy package whose address isn't recorded in its lockfile).

In the dependency graph, we store a dependency graph for each environment. See the Move.lock example
above.

## From start to package graph

0. validate manifest
    - parsing against schema
    - TODO: maybe allow environments that aren't listed - this will allow package system to support
      special verifier or testing environments. Maybe warn on unrecognized environments. Similarly,
      there may be test-related reasons to include a dependency in "dep-replacements" that wasn't
      listed in "dependencies".

1. check for repin
    - fetch all pinned dependencies
    - walk pinned graph - for each node, if manifest doesn't match digest, need to repin
    - if you don't need to repin, you're done

    - TODO: what if someone has mucked around with the lockfile? Maybe worth doing some validation
      if we're not repinning?

2. recursively repin everything
    - explode the manifest dependencies so that there is a dep for every environment
        - include implicit dependencies
    - resolve all direct external dependencies into internal dependencies
    - fetch all direct dependencies
        - note that the lockfiles of the dependencies are ignored
    - recursively repin dependencies
        - note that we will need to pay attention to `use-environment` here
    - record FS path, pinned dep info, manifest digest

3. check for conflicts
    - if a node doesn't have a published id for a given network, warn: you won't be able to publish

    - if two nodes have same original id and same published id (but different source), choose one
      using heuristics. E.g. choose a source dep over a bytecode dep. Maybe also warn?

    - if two nodes have same original id and different published id for a given network, there is a
      conflict - warn

4. rewrite the dependency graph to the lock file

## Invariants

To the best of our ability, we establish the following invariants while constructing the package
graph:

 - there is a node in the `pinned` section having an ID that matches the package name and a `source`
   field containing exactly `{ local = "." }`; we call this the "root node"

 - the dependency graph in `pinned` is a DAG rooted at the root node.

 - every entry in `pinned` has a different `source`

 - For each node `p` in the `pinned` section of `Move.lock`:
     - `p.manifest_digest` is the digest of `Move.toml` for the corresponding package
     - `p.deps` contains the same keys as the dependencies of `Move.toml`
     - `p.source` has been cached locally

 - environments are in Move.lock if and only if they are in Move.toml; all other environments live
   in .Move.<env>.lock. If this is violated, we move the published metadata into or out of Move.lock

These can also be violated if a user mucks around with their lockfiles - I think we should just do
best-effort on that. We may provide an additional tool to help fix things up (e.g. `sui move
sync-lock` or something).

## Resolution

We maintain the distinction between “internal” and “external” resolvers, but the job of an external
resolver is much simpler than the current design - it is simply responsible for converting its own
dependencies into internal dependencies (currently git, local, or on-chain).

External resolvers may want to return different information for each network (e.g if there are
different versions published on mainnet or testnet). They may also want to batch their lookups. To
support this and possible future extensions, we use JSON rpc (over stdio) for communication between
the package management system and external resolver binaries.

To be concrete, if there are any dependencies of the form `{r.<res> = <data>}` we will invoke the
binary `<res>` from the path with the `--resolve-deps` command line argument. We will pass it a JSON
RPC batch request on standard in and read a batch response from its standard out. The batch request
will contain one call to the `resolve` method for each environment and for each dependency, passing
the environment and the `<data>` as arguments. The method call responses will then be decoded as
internal dependencies.

Currently the only external resolver is mvr, and it will just look up the mvr name and convert it
into a git dependency.

## Implicit dependencies

Unlike the current system, explicitly including an implicit dependency is an error; you disable
implicit dependencies by adding `implicit_dependencies = false` to the `[package]` section of your
manifest.

Like externally resolved dependencies, implicit dependencies will be pinned to different versions
for each environment.

## Fetching

Once dependencies have been pinned they should be fetched to the local cache in `.move`. Since
dependencies are pinned, we don’t need to keep around a git repository for the cache - the cache is
simply a snapshot of the files.

In particular, we should use sparse shallow checkouts to make downloading fast.

Since the cached files will never change, it is safe to use them across projects. In particular, I
think we don’t need to have a copy of dependency sources within a project’s build artifacts.
However, we may want to mark the files as read-only in this case, since the LSP may end up
navigating to them during debugging and accidental changes could be bad.

Of course, the users may choose to update the cache anyway (for example if they want to locally test
a change). We should only allow building with a dirty cache if the user gives a special command-line
argument.

Note that since we always fetch after pinning, we never need to go to the network unless a
dependency needs to be repinned (because the manifest changed or the user ran update-deps).

### Bytecode dependencies

(not part of MVP)

Bytecode dependencies are handled entirely during the fetching process. When fetching a bytecode
dependency, we immediately convert it into a source dependency by generating stubs, a manifest, and
lockfiles.

Once this process is complete, bytecode deps are identical to any other dependency.

The only other place where bytecode deps are relevant is during testing - we need to make sure that
we run the stored bytecode rather than the compiled stubs. However, it is probably best to always
use the cached build artifacts when testing.

## Validation

TODO: this maybe needs to change

Finally, we may want to do some validation and sanity checks after downloading. For example, `mvr`
would check the `published-at` fields of the dependencies' lock files match the registered versions.
Additionally, we perform other checks. We can abstract this as a "validation" step, where we run all
validators; mvr could supply an external validator.

For example, we can perform the following checks:

 - do the (transitive) dependencies have published versions on all of the defined environments?
    > Warning: your package depends on foo but foo's Move.lock does not
    indicate that it is published on mainnet.
    >
    >
    > If the package is published, you can specify the address in your `Move.toml`
    > in the [dep-replacements] section:
    >
    > [dep-replacements]
    > foo = { published-at = "0x....", original-id = "0x..." }
    >
    > here published-at should refer to the current version of the package and
    > original-id should refer to the first version of the package.
    >

 - are the chain IDs for the environments consistent?

    > Warning: your package depends on foo but foo’s devnet chain ID (defined in foo/Move.toml) is
    > different from your chain ID (in Move.toml). You may need to change the chain ID for devnet in
    > your Move.toml or update foo using
    >
    >   sui move update-deps foo
    >

- If the dep-replacements do specify a published-at / original-id, does it match
the one in the package?

    > Warning: your package specifies a published-at address for foo on
    > mainnet, but package foo is published at a different address according to its Move.lock file.
    > Consider removing the published-at field for foo from your Move.toml file

- If two dependencies in the graph have the same original ID but different published addresses, then
  there is a conflict - they are referring to different versions of the same package.

    > Error: packages foo and bar depend on different versions of baz
    >
    >
    > You must choose a specific version of `baz` by giving an override dependency:
    >
    > [dependencies]
    > baz = { <info for latest of the conflicting versions>, override = "true" }
    >

- If two deps have the same published addresses, then we have two source packages claiming to match
  the same on-chain package. We will build with two different packages, as specified by the
  dependencies.

- Are there local dependencies that don’t share a repository with the current package? This is
  almost certainly a bug!

    > Warning: you have declared a local dependency on “../../../other-package/” but
    > “../../../other-package/” is not contained in your repository. This can lead to version
    > inconsistencies during publication and is strongly discouraged.
    >

- Naming consistency: do names declared by dependencies match the names given to them by dependents?
  (Also check if package name is different from its dependencies’ names)

    > Error: the package referred to by `foo` in `Move.toml` is actually named `bar` . If this is
    > intentional, add a `rename-from` field to the dependency line in `Move.toml`:
    >
    >   foo = { …, rename-from = “bar” }
    >

In general, any property that would cause a publish to fail should probably be reported either at
update or at build time.

# User operations

## Update dependencies (repinning)

If the user runs `sui move update-deps`, we rerun resolution, pinning, fetching, and validation for
all dependencies. If they run `sui move update-deps d1 d2` we rerun these steps only for the
specified dependencies.

## Build / Test

Building and testing for a given environment is easy - once we reestablish the invariants, all the
source packages for the pinned dependencies are available. We always compile and test against the
source/bytecode we have.

One important note is that we have removed named addresses - packages will use the names assigned in
the `[dependencies]` section of their toml files to address packages. As a sanity check, we will
ensure during validation that the package names declared by dependencies either match the declared
name of the dependency in the dependent package or the `rename-from` field.

If the package declares multiple environments, we build and/or test for all environments unless the
user provides a flag telling us otherwise. Although less efficient, it ensures that the user detects
problems early. Compilation and testing are currently cheap since packages are typically small.

Care must be taken to use the correct environments if a package has a dependency with a
`use-environment`

## Publish / Upgrade

TODO: report on changed dependencies during upgrades

During publication, we need to recheck that the dependencies are all published on the given network
and that the relevant chain IDs are consistent.

During upgrade we can use the stored upgrade cap.

We also need to update the lock file to include the updated `PublishedMetadata`.

There are some footguns we want to prevent. For example, if a user has two different environments
for the same chain ID, we should require them to specify a specific environment (this would prevent
accidentally publishing an alpha version to the beta address, for example).

Although we certainly want to remove `--with-unpublished-deps` (which has bad semantics), there is a
useful feature we might provide `--publish-unpublished-deps=env_name`. This would work by recursively
publishing any packages that haven't yet been published. This would be a nice convenience for
working with a group of related packages (esp. during testing). We should only allow it for local
ephemeral environments and devnet, since publication relies on the user to update the lockfiles of the
published packages; for non-ephemeral networks these would be sitting in the cache and not easy to
push back upstream. Put another way, the `Move.lock` files in the cache should be read-only (but the
`.Move.<env>.lock` files can be created/updated).

Some users will want to perform the actual publication step outside of the CLI (for example if they
want to go through a separate signing process for the transaction). For these transactions, I don't
think we know the published address until after the transaction executes. I think we should still
update the lock file and write "TODO"s for the fields that we don't know. We might also provide a
way for users to replace the TODOs their lock file given the transaction ID of a publication
transaction.

```toml
[published.mainnet]
...
published-at = "TODO: replace this with the published address of your package"
...

```

The user can still screw this up, but they at least have the chance of noticing the problem and
fixing it when they review the commit. We can also output a loud warning from the command line!

## Post-MVP operations

### Fixup lockfiles

If a user somehow loses track of publication information or wants to migrate an old package, we
should provide a tool to allow them to add a PublishedMetadata entry to their lockfile (possibly
using a transaction ID?).

We might also provide tools to fix things up if the user somehow ends up with two PublishedMetadata
for the same environment (one in the main lockfile and one in .Move.<env>.lock).

### Verify

Source verification is the process of checking that recompiling a given source package produces a
given binary output.

Our goal for this MVP is not to reimplement source verification correctly, but rather to store
enough information in the lock file that we (or someone) can reliably perform source verification in
the future.

We envision an architecture where each version of the compiler defines a build config format and has
a code generation module that, given a build config and source code (including dependency sources),
will always produce the same compiled bytecode.

For a given `PublishedMetadata`, we can use the `toolchain-version` to determine which code
generation module to use; this code generation module can then use the stored pinned dependencies
and the `build-config` field to reliably reproduce the bytecode (which can then be compared to the
on-chain version).

# Migration and backwards compatibility

For backwards compatibility, we need to be able to mix packages that use the old-style lock and
manifest format with the formats described here.

For migration, we need to be able to assist a user in upgrading their package to the new format.

In both cases, we need to be able to map an old-style package into a new-style package, but they are
different because for compatibility we need something fully automated (but that only needs to work
on a single chain), while for migration we can do a better job using user assistance.

More research is needed to describe the exact mapping, but the goal is that we unpack old-style lock
files into the same data structure as new-style lock files.

## Build (manifest)

Another field that exists today is `build`. It particularly refers to an architecture or language
version. I believe we’re not using it in our Move style code, but we might have to consider it in
some way.

## Flavor (manifest)

In the current package management, one can declare the kind of flavor the Move package has by
passing in the `flavor` field, which is of type String. It can take `sui`, `core`, `aptos`, and some
other flavors. We’d need a way to keep a backward compatible way here to know which flavor was used.

## Named addresses (manifest)

The biggest migration challenge will be the removal of named addresses. We will have to look at a
lot of data to see how people are using named addresses outside of package management.

## Packages from before automated address management

TODO

## Packages using automated address management

TODO

## What we see in the wild

Note: @Manos Liolios promised me a particularly wild example or two, and a more comprehensive list
of examples post-bootcamp.

 - Crazy example: https://github.com/pyth-network/pyth-crosschain/tree/main/target_chains/sui/contracts
 - Left this in the tooling meeting notes, but for anyone else interested, this is the SuiGPT Move
   Dataset v1:
   [https://github.com/CMU-SuiGPT/sui-move-dataset-v1](https://github.com/CMU-SuiGPT/sui-move-dataset-v1)
   This includes Move 2024 and legacy Move repos, all with Move.toml files - it's 7 months old, so
   if you want fresher repos than that, another alternative would be grabbing repos from the
   Electric Capital dataset.

# Post MVP ideas

- Allow using local package and dependency names in PTBs.
- Tools for querying dependencies (e.g. what environments does the package define?)
    - GraphQL queries over manifests?
- Deny lists?

# Other considerations

## Changes to Move

## Security considerations

## Impact on mvr

## Impact on our tests

## Impact on typescript SDK

## Impact on the packages we deploy (e.g. suifrens)

## IDE integration

## Pluggability in the new CLI

## Interaction with local forked networks

TODO: what about forking? Is publishing against a forked network something we even care about? If
so, does a forked network share a chain ID with the original? How do we distinguish packaged that
are published pre-fork from those published post-fork? There's a lot of complexity here.
