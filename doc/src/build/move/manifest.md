---
title: Move.toml
---

Every Move package has a `Move.toml` file as a manifest for the package. The Sui Move compiler creates the `Move.toml` file for you when you create a new package, but you might need to update the file throughout the lifespan of your package.    

The manifest contains information divided into three sections:
* `[package]`: Your package metadata, such as name and version.
* `[dependencies]`: List of packages that your package depends on. Initially, the Sui Framework is the only dependency, but you add third-party dependencies to this section as needed.
* `[addresses]`: A list of *named addresses*. You can use the names listed as convenient aliases for the given addresses in the source code.

## [package] section

The `[package]` section contains the following information:

* `name`: The name of your package. Created by Sui Move compiler.
* `version`: The current version of your package. The Sui Move compiler creates the first value.
* `published-at`: The published address of the package. The Sui Move compiler does not automatically create this entry.

### The published-at property

Package dependencies in `Move.toml` must include a `published-at` field that specifies the published address of the dependency. For example, the Sui framework is published at address `0x2`, so its `Move.toml` file includes the `published-at` entry:

```toml
published-at = "0x2"
```

If your package depends on another package, like the Sui framework, the network links your package against the `published-at` address specified by the on-chain dependency after you publish your package. When publishing, the compiler resolves all package dependencies (i.e., transitive dependencies) to link against. Consequently, you should only publish packages where all dependencies have a `published-at` address in their manifest. By default, the publish command fails if this is not the case. If necessary, you can use the `--with-unpublished-dependencies` flag with the publish command to bypass the requirement that all dependencies require a `published-at` address. When using `--with-unpublished-dependencies`, all unpublished dependencies are treated as if they are part of your package.

## [dependencies] section

The Sui Move compiler creates the `[dependencies]` section for you with a single entry for the GitHub address of the Sui network. If you need to use a local version of the network, you can edit the address to point to the local version. For example, 

```toml
Sui = { local = "../crates/sui-framework" }
```

You place each additional dependency your package has on a new line directly below the previous one.

## [addresses] section

The Sui Move compiler creates the `[addresses]` section for you with an entry for your package and one for the Sui network.

```toml
[addresses]
your_package_name =  "0x0"
sui =  "0000000000000000000000000000000000000000000000000000000000000002"
```

The aliases defined in `[addresses]` enable you to reference the shortened name instead of the actual address. For example, when you import from Sui in a module, you can write:

```rust
use sui::transfer
```

Without the alias, you have to write:

```rust
// You could also use 0x2
use 0000000000000000000000000000000000000000000000000000000000000002::transfer
```

Dependency address values can change at different stages of development, also, so using the alias means the change only needs to occur in the manifest.
 
