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

# Document status

 - 4/02/25 Removed `dev-dependencies` section from `Move.toml` schema. Added the
   `flavor` section in backwards compatibility.
 - 3/21/25 Final draft circulated for comments (minor changes)
 - 3/13/25 (incomplete) Rewrote based on initial [design document][notion-overview] and discussion.
     - Replaced local registry with Move.<environment>.lock
     - Separated resolution into resolution and pinning
     - Added verification step to pinning
     - Added concrete schemata
     - Removed comparisons with old system
     - Added change detection for Move.toml
 - 4/3/25 Updates based on discussions
     - added list of questions
     - changed resolver-specific manifest entries to use same format as today
     - added more detail to external resolver protocol
     - specified checkouts are sparse and shallow
 - 4/25/25
     - TODO:
         - [dep-overrides] is now [dep-replacements]
         - sui move build/test now use the default dependencies unless you ask otherwise
         - two packages share an identity if they have the same original-id in any environment
         - maybe need to do additional identity checks when running `--publish-unpublished-deps`
         - cached deps are read-write but you can’t use dirty ones without asking
         - non-git local deps are a loud warning
 - 5/5/25
     - Migrated to monorepo

# Points of contention and remaining questions

(*) indicates @Michael George ‘s preference

 - In manifest: how to name the environment-specific dependency overrides?
     - * opt 1: `[dep-replacements] mainnet.foo = {...}`
         - Cons: `override` has a different meaning inside of a dependency
     - opt 2: `[env.mainnet.dependencies] foo = {...}`
         - Cons: makes it seem like this might be all the dependencies on
           mainnet, rather than something that is merged into the default
           dependencies
     - **Resolution**: we’re using `[dep-replacements]`
 - For `move build` and `move test`: should we build/test for all environments or only for one?
     - * opt 1: Build/test for all environments by default
         - Con: efficiency, but compilation is fast
         - Con: Need to do work to deduplicate warnings from different environments
     - opt 2: Build/test for only current environment by default
         - Con: defers error detection
         - Con: can be easy to forget to test with a specific configuration
         - Con: more complicated CI (where presumably you do actually want to test everything)
     - **Resolution**: we’ll ignore `[dep-replacements]` in `build` and `test`
       unless you pass `--env` , i.e. we will use the default environment. We’ll
       ignore the client environment; we also allow `--all-envs` for `test`,
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
     - * opt 1: error
         - Cons: maybe there’s someone who really knows what there doing and
           just wants to publish the damn thing and doesn’t want anyone to ever
           depend on them
     - opt 2: warning
         - Cons: nobody can reliably depend on you or perform source
           verification. If you do screw this up, your dependents can’t easily
           do anything about it
     - **Resolution:** loud, clear warning that explains why this is bad
 - Include the dependencies in the publication records in Move.lock?
     - opt 1: include them

         ```toml
         [published.mainnet.dependencies]
         d1 = { git = "..." }
         d2 = { ... }
         ```

         - Pro: Keeps a “historical record”
         - Con: adds complexity to the lockfile; in particular there’s now two sets of env-specific
           deps (unpublished and published)
     - * opt 2: exclude them
         - Pro: The only purpose for these is source verification; presumably if you are verifying
           the source then you have the source, which includes the lockfile with its pinned deps.
           However these entries can cause confusion since you now have two sets of env-specific
           deps (the unpublished overrides section and the stamp for the most recent publication)
     - opt 3: stamp the current repo instead

         ```toml
         [published.mainnet]
         pinned-source: { git = "...", rev = "0xdeadbeef" }
         ```

         - Pro: gives a hook for a tool like mvr to derive the source versions from a single default version
         - Con: the stamp doesn’t show up until the commit after the one that is published. I don’t think this actually matters, since these are “historical records” anyway.
         - Con: not clear how to generate the pin (e.g. what if we want to make walrus the source of truth; if there are multiple remotes, how do we choose, etc)
 - Question: What should we name the new edition that contains these changes?
     - Move 2025!
 - Question: What does the `flavor` field currently do? How does it interact with `edition`? Do we need to support it?
 - Question: What is `--dependencies-are-root`? Do we still need it?
 - Question: Do we want a `build` section?
 - Question: `override = true` for transitive deps
     - If A depends on B and C, B depends on Dv1, C depends on Dv2, Dv1 depends on Ev1, and Dv2 depends on Ev2, does A need to specify an override for E?
     - If so, should we change this?
 - Worry: packages are now identified by published address instead of name. Published addresses are chain-specific, which is fine; we can detect errors separately on different chains. However, what about ephemeral networks and `--publish-unpublished-deps`? Is it ok that we just publish two different versions of a package and treat them as different packages (you would already have gotten a warning if they are published on one of the networks you care about).
 - Need to think more on: how do we help users who are publishing using mechanisms other than the CLI to keep their lock files up to date?

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
bar = { r.mvr = "@protocol/bar" }
baz = { ... }

[dep-replacements]
# used to replace dependencies for specific environments
mainnet.foo = {
	source = "git", repo = "...", # override the source of the dep
	original-id = "....", # add an explicit address; see Backwards Compatibility
	published-at = "...",
	use-environment = "mainnet_alpha" # override/specify the dep's environment
}

```

Move.lock (contains `unpublished` and entries for environments defined in `Move.toml`):

```toml
[move]
version = 4

[unpublished.dependencies.pinned]
# pinned transitive dependencies from most recent `sui move upgrade-deps`
# note: local deps are pinned as locals - the latest are always used
# see Pinning section
foo = { ..., rev = "deadbeef01234" }
bar = { ... }
baz = { ... }

[unpublished.dependencies.unpinned]
# these are the dependencies as written in Move.toml, used to detect changes
foo = { ..., rev = "releases/v3" }
bar = { external = "mvr", name = "@protocol/bar" }
baz = { ... }

[unpublished.dep-replacements.mainnet.pinned]
foo = { ... } # pinned versions of the dep-replacements
std = { ... } # system deps are chain-dependent so would be pinned here

[unpublished.dep-replacements.mainnet.unpinned]
foo = { ... } # unpinned versions of the dep-replacements from Move.toml

[published.mainnet] # metadata from most recent publish to mainnet
chain-id = "35834a8a"
published-at = "..."
original-id  = "..."
upgrade-cap = "..."

toolchain-version = "..."
build-config = "..."

[published.mainnet.dependencies]
# pinned transitive dependencies from most recent publish to mainnet
# these are only used for source verification
std = { ... }
sui = { ... }
foo = { ... }
bar = { ... }

[published.testnet] # metadata from most recent publish to testnet
chain-id = "4c78adac"
published-at = "..."
original-id = "..."
upgrade-cap = "..."
toolchain-version = "..."
build-config = "..."

[published.testnet.dependencies]
std = { ... }
sui = { ... }
foo = { ... }
bar = { ... }
baz = { ... }

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

[dependencies]
foo = { ... }
bar = { ... }
baz = { ... }

```

# Schema for manifest and lock files

```
Move.toml
    package
        name : PackageName
        edition : Edition # TODO: version bump? # TODO: optional?
        version : Optional String
        license : Optional String
        authors : Optional Array of String
        implicit_deps : Boolean (default true)

    environments : EnvironmentName → ChainID

    dependencies : PackageName → (SourceDependencyInfo + DependencyLocation)

    dep-replacements : (EnvironmentName | SpecialName) → PackageName → (Optional DependencySpec + Optional AddressInfo)

Move.lock
		move
	    version : Integer = 4

    unpublished
        dependencies
            pinned   : PackageName → PinnedDependencyLoc
            unpinned : PackageName → DependencyLocation
        dep-replacements : EnvironmentName →
            pinned   : PackageName → PinnedDependencyLoc
            unpinned : PackageName → DependencyLocation

    published : EnvironmentName → PublishedMetadata

Move.<EnvironmentName>.lock
    PublishedMetadata

SpecialName: "test" | "verify" | ...

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
    version : Int # used to detect dependency downgrading

    toolchain-version : Optional ToolchainVersion
    build-config : Optional OpaqueBuildConfig
    dependencies : PackageName → PinnedDependencySpec

```

# Internal Operations

## Overview

The purpose of the lockfile is to give stability to developers by pinning consistent dependency
versions. These pinned versions are then used by other developers working on the same project, by
CI, and so forth. Dependencies are only repinned in two situations:

1. The user explicitly asks for it by running `sui move update-deps`. This command will repin all
   dependencies (or you could ask to update a single dependency using `sui move update-deps
   dep-name`).
2. The user adds or modifies a dependency in their `Move.toml` file. We detect this by comparing
   `unpublished.dependencies.unpinned` in `Move.lock` with `dependencies` in `Move.toml`. If an
   entry is different, that means the user has modified it, so we repin it.

In addition to the main set of dependencies, the user can also override dependencies in specific
environments using a `[dep-replacements]` section. These are treated identically, except that they
are pinned in the `[unpublished.dep-replacements]` section instead of `[unpublished.dependencies]`
(the differences are also relevant for publication).

When we do detect that a dependency needs to be updated, we perform the following steps:

 - **Resolving** converts external dependencies internal dependencies
 - **Pinning** converts a dependency identifier that can change into a stable identifier
 - **Fetching** downloads the dependency and prepares it for use by the compiler
 - **Validation** checks that the entire set of dependencies are valid

These operations are described in the remainder of this section

## Invariants

To the best of our ability, we maintain the following invariants between `Move.toml` and `Move.lock`s:

 - unpublished.dependencies.unpinned matches manifest.dependencies (likewise for dep-replacements).
   If we discover a mismatch, we upgrade the mismatched dependency (as if the user ran `sui move
   upgrade-deps foo`
 - keys of unpublished.dependencies.pinned are the transitive closure of the dependencies in
   unpublished.dependencies.unpinned. This can change if a local dependency’s changes its transitive
   dependencies. Therefore, we always perform these checks on local dependencies as well as the main
   package
 - unpublished.dependencies.pinned are cached locally (transitively). If any are missing, we refetch
 - environments are in Move.lock if and only if they are in Move.toml; all other environments live
   in .Move.<env>.lock. If this is violated, we move the published metadata into or out of Move.lock

These can also be violated if a user mucks around with their lockfiles - I think we should just do
best-effort on that. We may provide an additional tool to help fix things up (e.g. `sui move
sync-lock` or something).

## Resolution

We maintain the distinction between “internal” and “external” resolvers, but the job of an external
resolver is much simpler - it is simply responsible for converting its own dependencies into
internal dependencies (currently git, local, or on-chain).

External resolvers may want to return different information for each network (e.g if there are
different versions published on mainnet or testnet). They may also want to batch their lookups.
Therefore, the interface for an external resolver is that it receives a map from chain IDs to names
to dependencies, and returns the same structure but with internal dependencies in the leaves.

To be concrete, if there are any dependencies of the form `{r.<res> = ...}`, we will invoke the
binary `<res>` from the path with the `--resolve-dependencies` command line argument, and pass the
dependency-specific fields (i.e. not the override flags, addresses, etc) in a toml-formatted stream
on standard in; we will then read a toml-formatted set of dependencies from standard out, then check
that the output has the same structure as the input (same chain IDs and package names) and that the
dependencies are internal.

Currently the only external resolver is mvr, and it will just look up the mvr name and convert it
into a git dependency.

Resolution is the phase where we handle implicit system dependencies - they are transformed to
normal dependencies. They are treated as dep-replacements since they are environment specific.

## Pinning

Pinning is the process of taking a user-specified dependency and converting it to a stable pinned
dependency. A pinned dependency is one that guarantees that future lookups of the same dependency
will yield the same source package. For example, a git dependency with a branch revision is not
pinned, because the branch may be moved to a different commit, which might have different source
code. To pin this kind of dependency, we must replace the branch name with a commit hash -
downloading the same commit should always produce the same source code.

One exception to this rule is local dependencies. Local dependencies within the same repository
should be pinned as local dependencies. That way, if a user has a repository containing both a
package and its dependency and they modify the dependency, the changes are picked up right away.

This exception means we should also process (transitive) local dependencies when we process the root
package. For example, we should repin if we detect changes in their lock files.

Pinned dependencies are stored in `Move.lock` in the `unpublished.dependencies.pinned` table.

## Fetching

Once dependencies have been pinned they should be fetched to the local cache in `.move`. Since
dependencies are pinned, we don’t need to keep around a git repository for the cache - the cache is
simply a snapshot of the files.

In particular, we should use sparse shallow checkouts to make downloading fast.

Since the cached files will never change, it is safe to use them across projects. In particular, I
think we don’t need to have a copy of dependency sources within a project’s build artifacts.
However, we may want to mark the files as read-only in this case, since the LSP may end up
navigating to them during debugging and accidental changes could be bad.

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
    > your Move.toml or upgrade foo using
    >
    >   sui move upgrade-deps foo
    >

- If the dep-replacements do specify a published-at / original-id, does it match
the one in the package?

    > Warning: your package specifies a published-at address for foo on
    > mainnet, but package foo is published at a different address according to its Move.lock file.
    > Consider removing the published-at field for foo from your Move.toml file

- is the dependency graph valid?

    > Error: packages foo and bar depend on different versions of baz
    >
    >
    > You must choose a specific version of `baz` by giving an override
    > dependency:
    >
    > [dependencies]
    > baz = { <info for latest of the conflicting versions>, override = "true" }
    >

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
upgrade or at build time.

# User operations

## Upgrade dependencies (repinning)

If the user runs `sui move upgrade-deps`, we rerun resolution, pinning, fetching, and validation for
all dependencies. If they run `sui move upgrade-deps d1 d2` we rerun these steps only for the
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

During publication, we need to recheck that the dependencies are all published on the given network
and that the relevant chain IDs are consistent.

During upgrade we can use the stored upgrade cap.

We also need to update the lock file to include the updated `PublishedMetadata`.

There are some footguns we want to prevent. For example, if a user has two different environments
for the same chain ID, we should require them to specify a specific environment (this would prevent
accidentally publishing an alpha version to the beta address, for example).

Although we certainly want to remove `--with-unpublished-deps` (which has bad semantics), there is a
useful feature we might provide `--publish-unpublished-deps`. This would work by recursively
publishing any packages that haven't yet been published. This would be a nice convenience for
working with a group of related packages (esp. during testing). We should only allow it for local
ephemeral environments, since publication relies on the user to update the lockfiles of the
published packages; for non-ephemeral networks these would be sitting in the cache and not easy to
push back upstream. Put another way, the `Move.lock` files in the cache should be read-only (but the
`.Move.<env>.lock` files can be created/updated)

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
