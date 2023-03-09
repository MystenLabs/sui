# The `published-at` property in `Move.toml`

Package dependencies are now required to specify a `published-at` field in `Move.toml` that specifies the addressed that the dependency is published at. For example, The SUI framework is published at address `0x2`. So, the `Move.toml` file for the SUI framework has a corresponding line that says:

```toml
published-at = "0x2"
```

The `published-at` field is used to compute and ensure that all of your package dependencies (i.e., transitive dependencies) exist on-chain. We recommend publishing packages where all dependencies have `published-at` values in their manifest, and publishing will fail by default if this is not the case. In special circumstances, use the `--with-unpublished-dependencies` flag with the publish command to manually bypass this check.
