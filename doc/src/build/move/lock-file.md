---
title: Move.lock
---

When you build a Move package, the process creates a `Move.lock` file at the root of your package. The file acts as a communication layer between the Move compiler and other tools, like chain-specific command line interfaces and third-party package managers. The `Move.lock` file contains information about your package, including its dependencies, that aids operations like verification of source code against on-chain packages and package manager compatibility.    

Like the [Move.toml](manifest.md) file, the `Move.lock` file is a text-based `toml` file. Unlike the package manifest, the `Move.lock` file is not intended for you to edit directly. Processes on the toolchain, like the Move compiler, access and edit the file to read and append relevant information. You also must not move the file from the root, as it needs to be at the same level as your `Move.toml` manifest. 

If you are using source control for your package, make sure the `Move.lock` file is checked in to your repository. This ensures every build of your package is an exact replica of the original.   

## On-chain source verification

`Move.lock` records the toolchain version and flags passed during package compilation so the compiler can replicate the process. This provides confirmation that the package found at a given address on chain originated from a specific source package. Having this data in the lock file means you don't have to manually provide this information, which would be time consuming and prone to error.

## Dependency resolution

When you build a package, the Move compiler resolves dependencies in `Move.toml`. After each resolution, the compiler writes the location of a dependency to the `Move.lock` file. If a dependency fails to resolve, the compiler stops writing to the `Move.lock` file and the build fails. If all dependencies resolve, the `Move.lock` file contains the locations (local and remote) of all your package's transitive dependencies. The `Move.lock` file stores the name and package details in an array similar to the following:

```
[[move.dependency]]
name = “A”
source = { git = “https://github.com/b/c.git”, subdir = “e/f”, rev = “a1b2c3” }

[[move.dependency]]
name = “B”
source = { local = “../local-dep” }

[[move.dependency]]
name = “C”
source = { git = “https://github.com/a/b.git”, rev = “d1e2f3”, subdir = “c” }

[[move.dependency]]
name = “D”
source = { address = “0xa1b2…3z” }
```


