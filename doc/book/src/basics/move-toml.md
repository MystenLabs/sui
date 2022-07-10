# Move.toml

Every Move package has a *package manifest* - it is placed in the root of the package. The manifest itself
contains a number of sections, main of which are:

- `[package]` - includes package metadata such as name and author
- `[dependencies]` - specifies dependencies of the project
- `[addresses]` - address aliases (eg `@me` will be treated as a `0x0` address)

```toml
{{#include ../../examples/Move.toml}}
```
