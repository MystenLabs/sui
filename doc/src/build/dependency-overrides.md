---
title: Dependency Overrides
---

One of the main features of the Move programming language used to program Sui smart contracts is code reuse. Packages implementing commonly used functionality, such as various cryptographic algorithms, can be simply reused by other developers that require this kind of functionality in their projects. As part of the package's lifecycle, its developers have the ability to [upgrade](./package-upgrades.md) a package to a newer version (e.g., to fix bugs or provide additional functionality), possibly multiple times. This naturally leads to a situation where some "user" packages may depend on one version of a "library" package while other "user" packages may depend on another. 

A package might depend on another package _directly_ or _indirectly_ (also called _transitively_). A direct dependency is specified in the "user" package's [manifest](./move/manifest.md) file and its version is directly controlled by the developer of the "user" package. Clearly these directly dependent packages might have dependencies of their own, and these dependencies become indirect dependencies of the "user" package if they are not also specified in the "user" package's manifest file.


As long as all versions of all (directly or indirectly) dependent packages are the same, no further action on the side of the "user" package's developer is required. Unfortunately, this is not always the case. Consider the following example of the `user` package directly depending on two "library" packages: `vault` and `currency` (these are not real packages and their URLs do not exist). The (simplified) manifest file of the `user` package would then resemble the following:

```move
[package]
name = "user"
version = "1.0.0"

[dependencies]
vault = { git = "https://github.com/vault_org/vault.git" }
currency = { git = "https://github.com/currency_org/currency.git" }
```

Further consider that both `vault` and `currency` depend on another "library" package, `crypto`, but each of them depends on a different version (`crypto` becomes an indirect dependency of the `user` package). Their respective manifest files could then resemble the following:

```move
[package]
name = "vault"
version = "1.0.0"

[dependencies]
crypto = { git = "https://github.com/crypto.org/crypto.git" , rev = "v1.0.0"}
```

```move
[package]
name = "currency"
version = "2.0.0"

[dependencies]
crypto = { git = "https://github.com/crypto.org/crypto.git" , rev = "v2.0.0"}
```

This situation represents the [_diamond dependency conflict_](https://jlbp.dev/what-is-a-diamond-dependency-conflict) where two indirect dependencies in turn depend on different versions of the same package. If all packages involved are developed independently, then the developer of the `user` package has a problem -- the Move compiler cannot discern which version of the `crypto` package to use and, as a result, building of the `user` package fails.

To resolve these types of problems, developers can _override_ indirect dependencies of the packages they develop. In other words, we provide a mechanism that the developer can use to specify a single version of a dependent package to be used during the build process. This can be done by introducing a _dependency override_ in the manifest file of the developed package. In the case of our running example, developer of the `user` package might decide that all dependent packages should use version `2.0.0` of the `crypto` package (note that it is the same as specified in the manifest file of the `currency` package but could be a version not used by any of the dependencies, for example `3.0.0`). At a technical level, in order to enforce a dependency override, a developer of a "user" package  has to put a dependency with a chosen version in their own manifest file, with the addition of the `override` flag set to `true`. The manifest file of the `user` package correcting the diamond dependency conflict in our running example would resemble the following:

```move
[package]
name = "user"
version = "1.0.0"

[dependencies]
vault = { git = "https://github.com/vault_org/vault.git" }
currency = { git = "https://github.com/currency_org/currency.git" }
crypto = { git = "https://github.com/crypto.org/crypto.git" , rev = "v2.0.0" , override = true }
```

Note that overriding transitive dependencies in such a manner might not always be successful. In particular, the `vault` package in our example might truly depend on the functionality of the `crypto` package that is only available in version `1.0.0` of this package (e.g, was removed in later package revisions), in which case the build would also be unsuccessful. By offering the ability to enforce dependency overrides we merely put another tool into the developer's toolbox in the hope that it would allow resolution of transitive dependency conflicts in the majority of cases.
