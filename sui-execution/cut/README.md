# Sui Execution: Cut

The `cut` binary is responsible for making copies of sets of crates
from the repository.  Use the `--dry-run` option to inspect the copies
that `cut` would make without committing them to the filesystem.

## Properties

When making copies, `cut` maintains the following properties:

- If crate A depends on crate B, then when copies A' and B' are made,
  A' will depend on B' (not B).
- If crate A (copied) depends on crate B (not copied) and refers to it
  by a relative path, the path in copy A' continues pointing to B.
- If crate C is a workspace member, then the copy, C', will also be
  added to the workspace members.
- If crate C is a workspace exclude, then the copy, C', will also be
  added to the workspace excludes.

## Finding Crates

The binary accepts two sets of configuration for discovering packages
to copy:

- A set of directories to search for crates, along with a destination
  directory to put copies of crates found within this directory, and
  an optional `suffix` parameter that is used to strip a common suffix
  from the names of crates found in this directory.
- A list of crate names to look for -- only crates whose names are in
  this list will be copied, and if a crate in this list is not
  encountered, a warning will be issued.

## Copying

When copying crates, the tool preserves the relative path of the crate
in its source directory. For instance, if crate A is found at path
`./foo/bar/baz/A` in its source directory, it will be copied to
directory `./foo/bar/baz/A` in its destination directory.

## Package Naming

`cut` accepts a `--feature` (or `-f`) parameter that is used to
disambiguate the names of copied crates from their originals.  When a
crate A with name `"a"` is copied as part of feature `foo`, its copy,
A', will have name `"a-foo"`.

## Root Detection

`cut` needs to know where the root of the repository is, to find the
manifest that contains the `[workspace]` configuration.  By default,
it sets this to the ancestor of the current working directory that
contains a `.git` directory.  If such an ancestor does not exist, or
it does not contain a `Cargo.toml` that contains a `[workspace]`
field, then the `--root` must be explicitly supplied.
