# Move.toml

Every Move package has a *package manifest* in the form of a `Move.toml` file - it is placed in the [root of the package](https://github.com/MystenLabs/sui/blob/main/doc/src/build/move/index.md#move-code-organization). The manifest itself contains a number of sections, primary of which are:

- `[package]` - includes package metadata such as name and author
- `[dependencies]` - specifies dependencies of the project
- `[addresses]` - address aliases (eg `@me` will be treated as a `0x0` address)

```toml
{{#include ../../examples/Move.toml}}
```
