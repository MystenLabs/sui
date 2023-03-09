# The `published-at` property in `Move.toml`

Package dependencies are now required to specify a `published-at` field in `Move.toml` that specifies the address that the dependency is published at. For example, The SUI framework is published at address `0x2`. So, the `Move.toml` file for the SUI framework has a corresponding line that says:

```toml
published-at = "0x2"
```

If your package depends on another package, like the SUI framework, your package will be linked against the `published-at` address specified by the SUI framework on-chain once you publish your package. When publishing, we resolve all of your package dependencies (i.e., transitive dependencies) to link against. This means we recommend publishing packages where all dependencies have a `published-at` address in their manifest. The publish command will fail by default if this is not the case. If needed, you may use the `--with-unpublished-dependencies` flag with the publish command to bypass the requirement that all dependencies require a `published-at` address. When using `--with-unpublished-dependencies`, all unpublished dependencies are treated as if they are part of your package.
