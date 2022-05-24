# workspace-hack

This `workspace-hack` is managed by [`cargo hakari`](https://docs.rs/cargo-hakari/latest/cargo_hakari/about/index.html)
which can be installed by:

```
cargo install --locked cargo-hakari 
```

If you've come here because CI is failing due to the workspace-hack package
needing to be updated you can run the following to update it:

```
cargo hakari generate # workspace-hack Cargo.toml is up-to-date
cargo hakari manage-deps # all workspace crates depend on workspace-hack
```
