# Move.toml

每个 Move 包都包括一个清单文件 `Move.toml`--它位于[包的根目录](https://docs.sui.io/build/move/index#move-code-organization)。清单本身包含了许多部分，其中最主要的是：
- `[package]` - 包括包相关的元数据，例如名字， 作者等
- `[dependencies]` - 声明项目的依赖
- `[addresses]` - 地址别名（例如 `@me` 是 `0x0` 的别名）

<details>
<summary>English Version</summary>

Every Move package has a *package manifest* in the form of a `Move.toml` file - it is placed in the [root of the package](https://docs.sui.io/build/move/index#move-code-organization). The manifest itself contains a number of sections, primary of which are:

- `[package]` - includes package metadata such as name and author
- `[dependencies]` - specifies dependencies of the project
- `[addresses]` - address aliases (eg `@me` will be treated as a `0x0` address)


</details>


```toml
{{#include ../../examples_zh/Move.toml.example}}
```
