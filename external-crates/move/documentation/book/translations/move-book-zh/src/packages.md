# 程序包(packages)

Packages allow Move programmers to more easily re-use code and share it
across projects. The Move package system allows programmers to easily:
* Define a package containing Move code;
* Parameterize a package by [named addresses](./address.md);
* Import and use packages in other Move code and instantiate named addresses;
* Build packages and generate associated compilation artifacts from packages; and
* Work with a common interface around compiled Move artifacts.

包允许 `Move` 程序员更轻松地重用代码并在项目之间共享。`Move` 包系统允许程序员轻松地：
* 定义一个包含 `Move`代码的包；
* 通过命名地址参数化包；
* 在其他 `Move` 代码中导入和使用包并实例化命名地址；
* 构建包并从包中生成相关的编译源代码；
* 使用围绕已编译 `Move` 工件的通用接口。

## 包布局和清单语法（Package Layout and Manifest Syntax）

A Move package source directory contains a `Move.toml` package manifest
file along with a set of subdirectories:

`Move` 包源目录包含一个`Move.toml`包清单文件以及一组子目录：

```
     a_move_package
        ├── Move.toml      (required)（需要的）
        ├── sources        (required)（需要的）
        ├── examples       (optional, test & dev mode)（可选的，测试 & 开发者模式）
        ├── scripts        (optional)（可选的）
        ├── doc_templates  (optional)（可选的）
        └── tests          (optional, test mode)（可选的，测试模式）
```

The directories marked `required` _must_ be present in order for the directory
to be considered a Move package and to be compiled. Optional directories can
be present, and if so will be included in the compilation process. Depending on
the mode that the package is built with (`test` or `dev`), the `tests` and
`examples` directories will be included as well.

标记为`required` 的目录必须存在才可以将该目录作为 `Move` 包并进行编译。可选目录被视为可存在的，如果存在，将包含在编译过程里。根据使用 （`test`或`dev`）构建包的模式，`tests`和`examples` 目录也将包含在内。

The `sources` directory can contain both Move modules and Move scripts (both
transaction scripts and modules containing script functions). The `examples`
directory can hold additional code to be used only for development and/or
tutorial purposes that will not be included when compiled outside `test` or
`dev` mode.

`sources`目录可以包含 `Move` 模块和 `Move` 脚本（事务脚本和包含脚本函数的模块）。`Example`目录可以保留仅用于开发和/或用作教程目的附加代码，当在 `test` 或者`dev`模式之外时，这些附加代码编译时不会被包括进来。

A `scripts` directory is supported so transaction scripts can be separated
from modules if that is desired by the package author. The `scripts`
directory will always be included for compilation if it is present.
Documentation will be built using any documentation templates present in
the `doc_templates` directory.

`scripts`目录是被支持的，如果包作者需要，事物脚本可以从模块中分离。如果该`scripts`目录存在，则编译时将始终包含该目录。
Move将使用存在于`doc_templates` 目录的任何模板构建文档。


### 包清单 Move.toml

The Move package manifest is defined within the `Move.toml` file and has the
following syntax. Optional fields are marked with `*`, `+` denotes
one or more elements:

Move 包清单在`Move.toml`文件中定义，并具有以下语法。可选字段标有`*`，`+`表示一个或多个元素：
```
    [package]
    name = <string>                  # e.g., "MoveStdlib"
    version = "<uint>.<uint>.<uint>" # e.g., "0.1.1"
    license* = <string>              # e.g., "MIT", "GPL", "Apache 2.0"
    authors* = [<string>]            # e.g., ["Joe Smith (joesmith@noemail.com)", "Jane Smith (janesmith@noemail.com)"]

    [addresses]  # (Optional section) Declares named addresses in this package and instantiates named addresses in the package graph
    # One or more lines declaring named addresses in the following format
    <addr_name> = "_" | "<hex_address>" # e.g., std = "_" or my_addr = "0xC0FFEECAFE"

    [dependencies] # (Optional section) Paths to dependencies and instantiations or renamings of named addresses from each dependency
    # One or more lines declaring dependencies in the following format
    <string> = { local = <string>, addr_subst* = { (<string> = (<string> | "<hex_address>"))+ } } # local dependencies
    <string> = { git = <URL ending in .git>, subdir=<path to dir containing Move.toml inside git repo>, rev=<git commit hash>, addr_subst* = { (<string> = (<string> | "<hex_address>"))+ } } # git dependencies

    [dev-addresses] # (Optional section) Same as [addresses] section, but only included in "dev" and "test" modes
    # One or more lines declaring dev named addresses in the following format
    <addr_name> = "_" | "<hex_address>" # e.g., std = "_" or my_addr = "0xC0FFEECAFE"

    [dev-dependencies] # (Optional section) Same as [dependencies] section, but only included in "dev" and "test" modes
    # One or more lines declaring dev dependencies in the following format
    <string> = { local = <string>, addr_subst* = { (<string> = (<string> | <address>))+ } }
```
An example of a minimal package manifest with one local dependency and one git dependency:

一个具有局部依赖项和一个 git 依赖项的最小包清单示例：
```
    [package]
    name = "AName"
    version = "0.0.0"
```

An example of a more standard package manifest that also includes the Move
standard library and instantiates the named address `Std` from it with the
address value `0x1`:

一个包括 Move 标准库并从中使用地址值`0x1`实例化命名地址`Std`的更标准的包清单示例：

```
    [package]
    name = "AName"
    version = "0.0.0"
    license = "Apache 2.0"

    [addresses]
    address_to_be_filled_in = "_"
    specified_address = "0xB0B"

    [dependencies]
    # Local dependency
    LocalDep = { local = "projects/move-awesomeness", addr_subst = { "std" = "0x1" } }
    # Git dependency
    MoveStdlib = { git = "https://github.com/diem/diem.git", subdir="language/move-stdlib", rev = "56ab033cc403b489e891424a629e76f643d4fb6b" }

    [dev-addresses] # For use when developing this module
    address_to_be_filled_in = "0x101010101"
  ```

Most of the sections in the package manifest are self explanatory, but named
addresses can be a bit difficult to understand so it's worth examining them in
a bit more detail.

包清单中的大部分段落都是不言自明的，但命名地址可能有点难以理解，因此值得更详细地检查它们。

## 编译期间的命名地址（Named Addresses During Compilation）

Recall that Move has [named addresses](./address.md) and that
named addresses cannot be declared in Move. Because of this, until now
named addresses and their values needed to be passed to the compiler on the
command line. With the Move package system this is no longer needed, and
you can declare named addresses in the package, instantiate other named
addresses in scope, and rename named addresses from other packages within
the Move package system manifest file. Let's go through each of these
individually:

回想一下，Move 具有命名地址，并且不能在 Move 中声明命名地址。正因为如此，到目前为止，命名地址及其值都需要在命令行上传递给编译器。但使用 Move 包系统时这将不再需要，您可以在包中声明命名地址，实例化范围内的其他命名地址，并从 Move 包系统清单文件中的其他包重命名命名地址，让我们分别来看看这些:

### 声明（Declaration）
Let's say we have a Move module in `example_pkg/sources/A.move` as follows:

假设我们有一个Move模块，`example_pkg/sources/A.move`如下所示:

```move
    module named_addr::A {
        public fun x(): address { @named_addr }
    }
```

We could in `example_pkg/Move.toml` declare the named address `named_addr` in
two different ways. The first:

我们可以用两种不同`example_pkg/Move.toml`的方式声明命名地址`named_addr`。首先：

```
    [package]
    name = "ExamplePkg"
    ...
    [addresses]
    named_addr = "_"
```

Declares `named_addr` as a named address in the package `ExamplePkg` and
that _this address can be any valid address value_. Therefore an importing
package can pick the value of the named address `named_addr` to be any address
it wishes. Intuitively you can think of this as parameterizing the package
`ExamplePkg` by the named address `named_addr`, and the package can then be
instantiated later on by an importing package.

声明`named_addr`为包`ExamplePkg`中的命名地址，并且 _该地址可以是任何有效的地址值_。因此，导入包可以选择命名地址的值作为`named_addr`它希望的任何地址。直观地，您可以将其视为通过命名地址`named_addr`参数化包 `ExamplePkg`，然后稍后通过导入包使包被实例化。

`named_addr` can also be declared as:

`named_addr`也可以声明为：

```
    [package]
    name = "ExamplePkg"
    ...
    [addresses]
    named_addr = "0xCAFE"
```

which states that the named address `named_addr` is exactly `0xCAFE` and cannot be
changed. This is useful so other importing packages can use this named
address without needing to worry about the exact value assigned to it.

这表明命名的地址`named_addr`是准确的`0xCAFE`并且不能更改。这很有用，因此其他导入包可以使用这个命名地址，而无需担心分配给它的确切值。

With these two different declaration methods, there are two ways that
information about named addresses can flow in the package graph:
* The former ("unassigned named addresses") allows named address values to flow
  from the importation site to the declaration site.
* The latter ("assigned named addresses") allows named address values to flow
  from the declaration site upwards in the package graph to usage sites.

使用这两种不同的声明方法，有关命名地址的信息可以通过两种方式在包图中流动：
*  前者（“未分配的命名地址”）允许命名地址值从进口站点流向申报站点。
*  后者（“分配的命名地址”）允许命名地址值从包图中的声明站点向上流动到使用站点。

With these two methods for flowing named address information throughout the
package graph the rules around scoping and renaming become important to
understand.

通过这两种在整个包图中流动命名地址信息的方法，了解范围和重命名的规则变得很重要。

## 命名地址的作用域和重命名（Scoping and Renaming of Named Addresses）

A named address `N` in a package `P` is in scope if:
1. It declares a named address `N`; or
2. A package in one of `P`'s transitive dependencies declares the named address
  `N` and there is a dependency path in the package graph between between `P` and the
  declaring package of `N` with no renaming of `N`.

在包`P`中的命名地址`N`如果满足以下条件，则在作用域内：

 1. 它声明了一个命名地址`N`；或者
 2. `P`的传递依赖项之一中的包声明了命名地址`N`，并且封装图在`P`和没有重命名的声明包`N`之间有一个依赖路径。

 Additionally, every named address in a package is exported. Because of this and
the above scoping rules each package can be viewed as coming with a set of
named addresses that will be brought into scope when the package is imported,
e.g., if the `ExamplePkg` package was imported, that importation would bring
into scope the `named_addr` named address. Because of this, if `P` imports two
packages `P1` and `P2` both of which declare a named address `N` an issue
arises in `P`: which "`N`" is meant when `N` is referred to in `P`? The one
from `P1` or `P2`? To prevent this ambiguity around which package a named
address is coming from, we enforce that the sets of scopes introduced by all
dependencies in a package are disjoint, and provide a way to _rename named
addresses_ when the package that brings them into scope is imported.

 此外，包中的每个命名地址都会被导出。由于这个和上面的范围规则，每个包都可以被视为带有一组命名地址，当包被导入时，这些地址将被带入作用域，例如，如果包`ExamplePkg`被导入，则该导入会将命名地址`named_addr`带入作用域。 因此，如果`P`导入两个包`P1`并且`P2`都声明了一个命名地址`N`，在`P`中则会出现以下问题：当`N`被引用于`P`时我们指的是哪个`N`？来自`P1`或来自`P2`的`N`？ 为了防止命名地址来自哪个包的这种歧义，我们强制一个包中所有依赖项引入的范围集是不相交的，并提供一种在将命名地址带入范围的包被导入时重命名命名地址的方法。

Renaming a named address when importing can be done as follows in our `P`,
`P1`, and `P2` example above:

导入时重命名一个命名地址可以在我们的`P`,`P1`和`P2`上面的示例中完成：
```
    [package]
    name = "P"
    ...
    [dependencies]
    P1 = { local = "some_path_to_P1", addr_subst = { "P1N" = "N" } }
    P2 = { local = "some_path_to_P2"  }
```
With this renaming `N` refers to the `N` from `P2` and `P1N` will refer to `N`
coming from `P1`:

这种重命名`N`指的是`P2`中的`N`并且`P1N`将指 `P1`中的`N`：
```
    module N::A {
        public fun x(): address { @P1N }
    }
```
It is important to note that _renaming is not local_: once a named address `N`
has been renamed to `N2` in a package `P` all packages that import `P` will not
see `N` but only `N2` unless `N` is reintroduced from outside of `P`. This is
why rule (2) in the scoping rules at the start of this section specifies a
"dependency path in the package graph between between `P` and the declaring
package of `N` with no renaming of `N`."

重要的是要注意 _重命名不是局部的_：一旦一个命名地址`N`在一个包`P`中被重命名为`N2`，所有导入`P`的包都不会看到`N`但只会看到`N2`，除非`N`是从`P`外引入的。这就是为什么本节开头的范围规则中的规则 (2) 特别说明了“在`P`和没有重命名的声明包`N` 的封装图中的依赖路径” 。

### 实例化（Instantiation）

Named addresses can be instantiated multiple times across the package graph as
long as it is always with the same value. It is an error if the same named
address (regardless of renaming) is instantiated with differing values across
the package graph.

只要命名地址始终具有相同的值，就可以在封装图中多次实例化命名地址。如果在整个封装图中使用不同的值实例化相同的命名地址（无论是否重命名），则会出现错误。

A Move package can only be compiled if all named addresses resolve to a value.
This presents issues if the package wishes to expose an uninstantiated named
address. This is what the `[dev-addresses]` section solves. This section can
set values for named addresses, but cannot introduce any named addresses.
Additionally, only the `[dev-addresses]` in the root package are included in
`dev` mode. For example a root package with the following manifest would not compile
outside of `dev` mode since `named_addr` would be uninstantiated:

只有当所有命名地址都解析为一个值时，才能编译 Move 包。如果包希望公开未实例化的命名地址，则会出现问题。这就是`[dev-addresses]`段要解决的问题。此段可以设置命名地址的值，但不能引入任何命名地址。此外， `dev`模式下仅根包中的`[dev-addresses]`会被包括进来。例如，具有以下清单的根包将不会在`dev`模式之外编译，因为`named_addr`不会被实例化：
```
[package]
name = "ExamplePkg"
...
[addresses]
named_addr = "_"

[dev-addresses]
named_addr = "0xC0FFEE"
```
## 用法、源代码和数据结构（ Usage, Artifacts, and Data Structures）

The Move package system comes with a command line option as part of the Move
CLI `move <flags> <command> <command_flags>`. Unless a
particular path is provided, all package commands will run in the current working
directory. The full list of commands and flags for the Move CLI can be found by
running `move --help`.

Move 软件包系统带有一个命令行选项，作为 Move CLI 的一部分move `<flags> <command>` `<command_flags>`。除非提供特定路径，否则所有包命令都将在当前工作目录中运行。可以通过运行`move --help`找到 Move CLI 的命令和标志的完整列表。

### 用法（Usage）

A package can be compiled either through the Move CLI commands, or as a library
command in Rust with the function `compile_package`. This will create a
`CompiledPackage` that holds the compiled bytecode along with other compilation
artifacts (source maps, documentation, ABIs) in memory. This `CompiledPackage`
can be converted to an `OnDiskPackage` and vice versa -- the latter being the data of
the `CompiledPackage` laid out in the file system in the following format:

一个包可以通过 Move CLI 命令，或是当作Rust函数`compile_package`的库命令来编译。 这种编译方法将创建一个编译包`CompiledPackage` 保存已编译的字节码以及其他编译内存中的源代码（源映射、文档、ABIs）。这个`CompiledPackage`可以转换为`OnDiskPackage`，反之亦然——后者是文件系统中的编译包 `CompiledPackage`数据，它的格式如下：

```
a_move_package
├── Move.toml
...
└── build
    ├── <dep_pkg_name>
    │   ├── BuildInfo.yaml
    │   ├── bytecode_modules
    │   │   └── *.mv
    │   ├── source_maps
    │   │   └── *.mvsm
    │   ├── bytecode_scripts
    │   │   └── *.mv
    │   ├── abis
    │   │   ├── *.abi
    │   │   └── <module_name>/*.abi
    │   └── sources
    │       └── *.move
    ...
    └── <dep_pkg_name>
        ├── BuildInfo.yaml
        ...
        └── sources
```

See the `move-package` crate for more information on these data structures and
how to use the Move package system as a Rust library.

有关这些数据结构和如何将 Move 包系统用作 Rust 库的更多信息，请参阅 `move-package` 箱（crate） 。
