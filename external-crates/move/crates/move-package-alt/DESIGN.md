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

# Example manifest and lock files

Move.toml:

```toml
[package]
name = "example"
edition = "2024" # we do not bump the edition just for package system changes
...

[environments]
# the [environments] section contains entries mapping names to chain IDs
#  mainnet = "35834a8a"
#  testnet = "4c78adac"
# but these two environments (mainnet and testnet) are added implicitly, so in
# most cases the `[environments]` section is not needed
#
# one potential use for additional environments is if you want to maintain
# multiple deployments on the same network; you could then use
# [dep-replacements] to have different dependencies for the different deployments
testnet_alpha = "4c78adac"
testnet_beta  = "4c78adac"

[dependencies]
foo_1 = {
	rename-from = "foo",                # needed for name consistency - see Validation section
	override = true,                    # same as today - see Linking
	git = "https://.../foo1.git",       # resolver specific fields
	rev = "releases/v4",
}

foo_2 = {
	rename-from = "foo",          # needed for name consistency - see Validation section
	git = "https://.../foo2.git", # resolver specific fields
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

Move.lock contains information about pinned dependencies for each environment:

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
#
# There is also a node for the current package; the only thing that makes it special is that it has
# the name of the current package as its identity (it will also always have `{ root = true }` as its
# source)
[pinned.mainnet.example]
source = { root = true, use-environment = "mainnet" }
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
source = { git = "...", path = "...", rev = "bade", use-environment = "mainnet_alpha" }
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

```

Move.published contains information about historical publications in each environment:

```toml
# This file should be checked in

[published.mainnet] # metadata from most recent publish to mainnet
# generic move fields:
chain-id = "35834a8a" # used to ensure the chain ID is consistent
published-at = "..."
original-id  = "..."
version = "3"

# other useful chain-specific stuff:
upgrade-cap = "..."
build-config = { ... }
toolchain-version = "..."

[published.testnet] # metadata from most recent publish to testnet
chain-id = "4c78adac"
published-at = "..."
original-id = "..."
version = "5"

upgrade-cap = "..."
toolchain-version = "..."
build-config = "..."
```

# Schema for manifest and lock files

See [src/schema](src/schema) for the schemata implementations.

```
Move.toml
    package
        name : PackageName
        edition : "2025"
        version : Optional String
        license : Optional String
        authors : Optional Array of String
        system_dependencies : Optional Array of String (default None)

    environments : EnvironmentName → ChainID

    dependencies : PackageName → DefaultDependency

    dep-replacements : EnvironmentName → PackageName → ReplacementDependency

DefaultDependency: # information used to locate a dependency
    # dep-type specific fields e.g. git = "...", rev = "..."
    override: bool
    rename-from: PackageName

ReplacementDependency:
    optionally any DefaultDependency fields
    optionally both AddressInfo fields      # see backwards compatibility
    optionally
    use-environment : Optional EnvironmentName

Move.lock
    move
        version : 4

    pinned : EnvironmentName -> PackageID →
        source : PinnedDependency
        manifest_digest : Digest
        deps : PackageName → PackageID

PinnedDependency:
    DependencyLocation with additional constraints - see Pinning

Move.published
    published : EnvironmentName →
        chain-id: EnvironmentID
        published-at: Object ID
        original-id: Object ID
        version: uint

        # sui specific
        upgrade-cap: Optional Object ID
        toolchain-verison: String
        build-config: table


AddressInfo:
    published-at : ObjectID
    original-id  : ObjectID

```

# Internal Operations

## Environments

All operations are performed in the context of a specific environment. From the package management
perspective, the environments must be declared in the manifest (although we provide default
environments for mainnet and testnet). The CLI may support other environments - for example
publishing to localnet must be supported, but in this case the user must supply `--build-env` to
indicate which manifest environment to use.

From here on, when we say "all dependencies", we are referring to dependencies in the current
environment.

From the CLI perspective, we have the problem of selecting an environment to use if the user doesn't
provide one. By default we will use the current chain ID from `sui client` (which we will keep
cached to support offline builds) to detect the correct environment from the manifest to use. By
using the chain ID we decouple the client environment names (which are really RPC specific) from the
manifest environment names, but we make things work out in the common cases of mainnet and testnet.

In our current MVP we do not do any all-environment operations. However, we could consider doing
some cross-environment sanity checks (such as detecting packages that conflict in one environment
but not another, or running tests in all environments.

## Pinned dependencies

The purpose of the lockfile is to give stability to developers by pinning consistent (transitive)
dependency versions. These pinned versions are then used by other developers working on the same
project, by CI, by source verifiers, and so forth.

A pinned dependency is one that guarantees that future lookups of the same dependency
will yield the same source package. For example, a git dependency with a branch revision is not
pinned, because the branch may be moved to a different commit, which might have different source
code. To pin this kind of dependency, we must replace the branch name with a commit hash -
downloading the same commit should always produce the same source code.

Local dependencies of git dependencies are pinned as git dependencies with the paths fixed up, but
local dependencies of the root package are pinned as they are.

Although we expect pinned dependencies to not change, there are a few situations where they could.
One possiblity is local dependencies - if a single repository contains both a dependency and the
depending project, we would want changes in the dependency to be reflected immediately in the
depending project. Moreover, this doesn't cause inconsistencies in CI because presumably those
changes would be committed together. Another example would be if a user wants to temporarily change
a cached dependency during debugging to rerun a test or something. To handle these situations, we
will store digests of all transitive dependency manifests and repin if any of them change.

Dependencies are always pinned as a group and are only repinned in two situations:

1. The user explicitly asks for it by running `sui move update-deps`. This command will repin all
   dependencies for the current environment.

2. If the parts of the manifest that are relevant for the current environment have changed, then all
   dependencies are repinned.

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

## Selecting an environment

In the new system, each environment potentially represents an entirely different dependency graph,
so the environment that we choose matters a great deal, and we must always be operating in the
context of an environment. However, we'd like the environment selection process to be as unintrusive
as possible (while still doing the "right thing").

The first principle of environment selection is that all commands (build, test, publish, etc) should
select environments in the same way.

The second principle is that we should only allow publishing to a chain with a chain ID that matches
the chain that the dependencies were published for and that the root package was built for (although
see below for localnet publication).

Since the established publication workflow publishes publishes to the chain identified by the active
CLI environment, we use the active environment to select the environment to build for. However, we
need to be careful because while the manifest environment names are global and have associated chain
IDs (in the sense that they are contained in manifest files and used by dependents), CLI
environments are local to a machine and may encapsulate things like different RPC endpoints as well
as the actual underlying chain.

Putting these constraints together leads to a somewhat complicated process for selecting the build
environment. Suppose that the active CLI environment is named "e" and that the chain ID for the
associated RPC is "i"[^cached-chain-id] (and the user hasn't provided a command-line argument to
override):


1. If the exact pair ("e", "i") is contained in the `[environments]` table of the manifest, we use
   that as the build environment

2. If an environment named "e" exists in the manifest but the associated chain ID "i'" doesn't match, we
   produce an error (perhaps this could be a warning during build/test and an error on publish).
   This can happen if the user messes up their manifest or local environment, or if the network has
   been wiped and restarted
    > Error: Environment `e` has chain ID `i` in your local environment, but `Move.toml` expects `e`
    > to have chain ID `i'`; this may indicate that `e` has been wiped or that you have a
    > misconfigured environment.

3. If no environment named "e" exists in the manifest, we use the chain ID to try to select the
   correct environment. This situation might happen e.g. if the user decides to call their local
   environment `main` instead of `mainnet`, or if they have `testnet_1`, `testnet_2`, etc. If there
   is a _unique_ environment named `e'` with chain ID `i`, we choose that environment and emit an
   info message.
    > Note: `Move.toml` does not define an `e` environment; building for `e'` instead

4. If there are multiple environments `e1`, `e2` in the manifest having chain ID `i`, we produce an
   error, requiring the user to specify the build env. This may happen if there are `testnet_alpha`
   and `testnet_beta` environments in the manifest and the user's active CLI environment is
   `testnet`.
    > Error: There is no `e` environment in the manifest, but environments `e1` and `e2` are
    > available. Run `sui move build --build-env e1` or `sui move build --build-env e2`

5. If there are no environments in the manifest with chain ID `i`, then we assume `i` is an
   ephemeral network (e.g. `localnet` or `devnet`). If `Pub.e.toml` exists, we will use the
   `build-env` from that file (Note that if the chain-id in `Pub.e.toml` doesn't match and we are
   publishing, we will get warning or error from the chain-id consistency checks).

6. If there is no `Pub.e.toml` file, we produce an error, depending on the command:
   For build:
    > Error: Your active environment `e` is not present in `Move.toml`, so you must specify the
    > environment to use to determine dependencies. Pass `--build-env <env-name>`, e.g.
    >
    >   sui move build --build-env testnet
    >
    > Note: adding local networks to `Move.toml` is discouraged; see `sui client test-publish --help`
    > for information on managing local networks.

   TODO: this message might be confusing since we have implicit environments

   For publish:
    > Error: Your active environment `e` is not present in `Move.toml`, so you cannot publish to
    > `e`.
    >
    >   - If you want to create a temporary publication on `e` and record the addresses in a local
    >     file, use the `test-publish` command instead
    >
    >        sui client test-publish --help
    >
    >   - If you want to publish to `e` and record the addresses in the shared `Published.toml`
    >     file, you will need to add the following to `Move.toml`:
    >
    >        [environments]
    >        e = "i"
    >

[^cached-chain-id]: We want to support offline builds, which means we need to have the CLI locally
    cache the chain IDs for each environment. We will update that cache whenever we actually contact
    an endpoint and whenever we update the local set of environments (and of course we must make
    sure it is up-to-date before publishing)


## Modes

We had considered using an environments like feature to support things like test-only or spec-only
dependencies (like the `[dev-dependencies]` section in rust or the old system). However, this leads
to a lot of complexity around integrating environments and modes.

Instead, we add an optional `modes` field to each dependency which contains a list of the modes to
enable the dependency for (if missing, we include the dep for all modes).

The set of allowed modes should be determined the same way they are for the Move compiler.

The advantage of this approach is that in each environment, each dependency has a well-defined set
of modes, and the dependency graph for a particular mode is always a subgraph of the dependency
graph for environment ignoring modes. Therefore we don't have to worry about conflicts between
environment overrides and mode overrides.

Mode filtering happens after package graph generation but before compilation. Since linkage happens
after compilation, that means we won't introduce any artificial dependency conflicts by treating all
modes simultaneously for the earlier stages. Keeping all modes through pinning means that we can pin
once-and-for-all for each environment. It also means that we fetch test-only deps even when not
building for test, which is good because it means we support offline test builds even if you've only
done a non-test build before (or vice-versa).

## From start to package graph

1. check for repin
    - if there's no lockfile, you need to repin; continue to step 2
    - fetch all pinned dependencies from the lockfile
    - walk pinned graph - for each node, if manifest doesn't match digest, need to repin - continue
      to step 2
    - if you don't need to repin, you're done

2. recursively repin everything
    - explode the manifest dependencies so that there is a dep for every environment
        - include system dependencies
    - resolve all direct external dependencies into internal dependencies
    - fetch all direct dependencies
        - note that the lockfiles of the dependencies are ignored
    - recursively repin dependencies
        - note that we will need to pay attention to `use-environment` here
    - record FS path, pinned dep info, manifest digest

3. check for conflicts
    - ensure the graph is not cyclic

    - if a node doesn't have a published id for the current environment, warn: you won't be able to publish

    - if two nodes have same original id, there may be a version conflict (but we can't know for
      sure until after compilation - see linkage below); warn

4. rewrite the dependency graph to the lock file

## Invariants

To the best of our ability, we establish the following invariants while constructing the package
graph:

 - for each environment, there is a node in the `[pinned.<env>]` section having an ID that matches
   the package name and a `source` field containing exactly `{ root = true }`; we call this the
   "root node"

 - the dependency graph in `pinned` is a DAG rooted at the root node.

 - every entry in `pinned` has a different `(source`

 - For each node `p` in the `pinned` section of `Move.lock`:
     - `p.manifest_digest` is the digest of `Move.toml` for the corresponding package
     - `p.deps` contains the same keys as the dependencies of `Move.toml`
     - `p.source` has been cached locally

 - all environments in Published.toml or Move.lock are in Move.toml

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
into a git dependency (there is also a mock-resolver that echoes its input that is used for
testing).

## System dependencies

Unlike the current system, explicitly including a system dependency is an error; you disable
system dependencies by adding `system_dependencies = []` to the `[package]` section of your
manifest. Each flavor can specify its default system dependencies (for Sui, that's `sui` and `std`).
Non-default system dependencies can be specified like that: `system_dependencies = ["sui", "std", "sui_system"]`

Like externally resolved dependencies, system dependencies will be pinned to different versions
for each environment.

TODO: maybe this isn't necessary; we can just disable local deps in the monorepo and put them in explicitly:
> The default system deps for Sui would be `sui` and `std`. The available system deps are `std`,
> `sui`, `system`, `deepbook-v2`, `bridge`, `monorepo-sui`, `monorepo-std`. The `monorepo` deps are
> converted to local dependencies are are used for our internal tests (they would expand to `sui = {
> local = "path_to_monorepo/crates/sui-framework/packages/sui" }` and would fail if they are used
> outside the monorepo.

## Fetching

Once dependencies have been pinned they should be fetched to the local cache in `.move`. Since
dependencies are pinned, we don’t need to keep around a git repository for the cache - the cache is
simply a snapshot of the files.

In particular, we use sparse shallow checkouts to make downloading fast. This requires care when
multiple projects live in the same repo, but there is no fundamental problem.

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

Care must be taken to properly generate dependencies. In particular, we should use the bytecode to
determine the direct dependencies rather than relying on the on-chain linkage table. See Linkage
below.

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
    > If the package is published, you should ask the author to add the address to the
    > `Move.published` file, but you can work around this by adding the address in your `Move.toml`
    > in the [dep-replacements] section:
    >
    > [dep-replacements]
    > mainnet.foo = { published-at = "0x....", original-id = "0x..." }
    >
    > here published-at should refer to the current version of the package and
    > original-id should refer to the first version of the package.

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
    > mainnet, but package foo is published at a different address according to its Move.published file.
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

  Update: we can only perform this step _after_ compilation because whether an override is necessary
  or not depends on whether dependencies are direct or transitive and we can't know that (esp. for
  legacy packages) until after tree shaking. See Linkage

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

## Compilation

Although compilation itself is outside the purview of the package system, the set of packages that
we pass to the compiler depends on the package graph, and there are some important changes from the
previous system.

In particular, the previous system precomputed the overrides before handing packages to the
compiler, but this means that dependencies may be compiled inconsistently from how they were
compiled on-chain. Source validation doesn't even help here unless we are super-careful, because the
source code and compiled bytecode can match completely but the bytecode we generate for our internal
operations can differ (e.g. in the presence of macros).

Therefore, the new system will compile each package against the pinned dependency version that it
has. This means there may be multiple version of the same package in the package graph. We only
detect and disambiguate conflicts during the linkage step, _after_ compilation.

Another important difference is that in the old system packages inherit named addresses from their
dependencies. In the new system, if you refer to a package by name in your source code, you must
specify a direct dependency in your manifest. This makes all dependency names local, which makes it
possible to integrate multiple packages with the same name.

## Linkage

For the entire process through compilation, the package system can treat the package graph as a
tree: there is no problem if multiple nodes represent "the same" package. However, before we can
publish or run a package, we must select a single implementation for each package original-id. This
is the process of linkage.

The rules for linkage and how they interact with overrides is the same as in the previous system,
although because we allow multiple packages with the same ids to appear in the compilation graph,
there are some corner cases that the old system gets a little bit wrong. In particular, the
definition of a valid linkage requires a distinction between direct and transitive dependencies, and
because named addresses are inherited in the old system it is possible to allow some linkages that
should require `override = true` in the manifests. There are also corner cases if a dependency is
declared but not used, but these only require people to write an additional unnecessary dep and so
aren't a large concern.

See the `test` module in [src/graph/linkage.rs] for a bunch of worked examples of linkage checking
(you can also render diagrams for the tests - see the comments there for instructions).

# User operations

## Update dependencies (repinning)

If the user runs `sui move update-deps`, we rerun resolution, pinning, fetching, and validation for
all dependencies. If they run `sui move update-deps d1 d2` we rerun these steps only for the
specified dependencies.

## Build / Test

Building and testing for a given environment is easy - once we reestablish the invariants, all the
source packages for the pinned dependencies are available. We always compile against the
source/bytecode we have.

When running tests, we compute the linkage first and then hand those packages to the VM for
execution. This models what happens on-chain as closely as possible.

## Publish / Upgrade

During publication, we need to recheck that the dependencies are all published on the given network
and that the relevant chain IDs are consistent. We also need to ensure that the additional on-chain
linkage requirements are met (for example, on-chain packages can have extra dependencies in their
linkage that they don't actually use, so we need to union in the on-chain linkages).

During upgrade we can use the stored upgrade cap.

We also need to update the `Move.published` file to include the updated `PublishedMetadata`.

There are some footguns we want to prevent. For example, if a user has two different environments
for the same chain ID, we should require them to specify a specific environment (this would prevent
accidentally publishing an alpha version to the beta address, for example).

Although we certainly want to remove `--with-unpublished-deps` (which has bad semantics), there is a
useful feature we might provide `--publish-unpublished-deps=env_name`. This would work by recursively
publishing any packages that haven't yet been published. This would be a nice convenience for
working with a group of related packages (esp. during testing). We should only allow it for local
ephemeral environments and devnet, since publication relies on the user to update the lockfiles of the
published packages; for non-ephemeral networks these would be sitting in the cache and not easy to
push back upstream. Put another way, the `Published.toml` files in the cache should be read-only (but the
`Pub.<env>.toml` files can be created/updated). (TODO: `Pub.<env>.toml` files aren't described yet).

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

### Verify source

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

### Pre-publication information dump

When publishing or upgrading a package, there are certain facts about the dependencies that can be
important for the user to be conscious of. For example, an upgrade can change the version of a
dependency, while a publication can produce a linkage that forces a dependency to upgrade. Although
the developer has to "sign off" on these (e.g. by adding `override = true` to the manifest or
running `upgrade-deps`), we think it makes sense to display these kinds of facts to the user as part
of the publication process.

# Migration and backwards compatibility

For backwards compatibility, we need to be able to mix packages that use the old-style lock and
manifest format with the formats described here.

For migration, we need to be able to assist a user in upgrading their package to the new format.

In both cases, we need to be able to map an old-style package into a new-style package, but they are
different because for compatibility we need something fully automated (but that only needs to work
on a single chain), while for migration we can do a better job using user assistance.

More research is needed to describe the exact mapping, but the goal is that we unpack old-style lock
files into the same data structure as new-style lock files.

## Unsupported features

We currently plan to remove support for the following features, and we don't mind if we break how
they work in legacy packages:

 - "wildcard" addresses: `foo = "_"`
 - `--with-unpublished-addresses`
 - `[dev-addresses]`
 - address renaming

## Build (manifest)

Another field that exists today is `build`. It particularly refers to an architecture or language
version. I believe we’re not using it in our Move style code, but we might have to consider it in
some way.

## Package names

To provide consistency between legacy and modern packages, we are attempting to extract a new-style
package name for legacy packages. This means, for example, that the `MoveStdlib` package should
actually be called `std` (so that its name in source code matches its package name, as with modern
packages).

To do this, we have a handful of heuristics for pulling the name from the named address table, using
the source code to disambiguate if necessary.

## Named addresses (manifest)

Although there's no facility for writing named addresses in the modern system, we're keeping around
the named addresses from legacy packages and using them. This means we need to perform a pass to
collect all of them from transitive dependencies. For legacy packages only, we
include these in the named addresses we hand to the compiler.

If a legacy package depends on a modern package, it will not inherit addresses from that package.

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

We will not make any changes to Move. A future version of Move may add restrictions that all modules
in the package's source code are defined in that package (right now you can write `module p::m` for
any `p`). The new package system will make this more sensible, but we don't plan to make any changes
now.

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
