In this test, the manifest has a dependency for the non-existent directory
"broken_dep_path", while the lockfile has that dependency pinned to "locked_dep_path" (which exists).

If you need to rebuild this example, here's how:

```sh
mv locked_dep_path broken_dep_path
cargo run move update-deps
mv broken_dep_path locked_dep_path
sed 's/broken_dep_path/locked_dep_path/' -i '' Move.lock
```

The last line updates the dependency in the lockfile so that it's not broken
