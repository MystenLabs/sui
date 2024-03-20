# 结构体和资源

A _struct_ is a user-defined data structure containing typed fields. Structs can store any
non-reference type, including other structs.

***结构体***（struct）是包含类型字段的自定义数据结构。结构体可以存储任何非引用类型，包括其他结构。

We often refer to struct values as _resources_ if they cannot be copied and cannot be dropped. In
this case, resource values must have ownership transferred by the end of the function. This property
makes resources particularly well served for defining global storage schemas or for representing
important values (such as a token).

如果结构值无法复制且无法删除，我们通常将其称为***资源***（resource）。在这种情况下，资源值必须在函数结束时转移所有权。这个属性使资源特别适合定义全局存储模式或表示重要的值（如 token）。

By default, structs are linear and ephemeral. By this we mean that they: cannot be copied, cannot be
dropped, and cannot be stored in global storage. This means that all values have to have ownership
transferred (linear) and the values must be dealt with by the end of the program's execution
(ephemeral). We can relax this behavior by giving the struct [abilities](./abilities.md) which allow
values to be copied or dropped and also to be stored in global storage or to define global storage
schemas.

默认情况下，结构体是线性的和临时的。我们的意思是它们：不能被复制，不能被删除，不能被存储在全局存储中。这意味着所有值都必须转移所有权（线性），并且必须在程序执行结束时处理这些值（临时）。我们可以通过赋予结构体允许复制或删除值以及将值存储在全局存储中或定义全局存储模式的[能力](./abilities.md)来放松这种行为（减轻限制）。

## 定义结构体

Structs must be defined inside a module:

结构体必须在模块内定义：

```move
address 0x2 {
module m {
    struct Foo { x: u64, y: bool }
    struct Bar {}
    struct Baz { foo: Foo, }
    //                   ^ 注意：尾随逗号是可以的
}
}
```

Structs cannot be recursive, so the following definition is invalid:

结构体不能递归（定义），所以下面的定义是无效的：

```move
struct Foo { x: Foo }
//              ^ 错误！ Foo 不能包含 Foo
```

As mentioned above: by default, a struct declaration is linear and ephemeral. So to allow the value
to be used with certain operations (that copy it, drop it, store it in global storage, or use it as
a storage schema), structs can be granted [abilities](./abilities.md) by annotating them with
`has <ability>`:

如上所述：默认情况下，结构体声明是线性且临时的。因此，为了允许将值用于某些操作（复制、删除、将其存储在全局存储中或将其用作存储模式），可以通过使用 `has <ability>` 标注它们来授予结构体[能力](./abilities.md)：

```move
address 0x2 {
module m {
    struct Foo has copy, drop { x: u64, y: bool }
}
}
```

For more details, see the [annotating structs](./abilities.md#annotating-structs) section.

有关更多详细信息，请参阅[标注结构体](./abilities.md#标注结构体)部分。

### 命名

Structs must start with a capital letter `A` to `Z`. After the first letter, constant names can
contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

结构体必须以大写字母 `A` 到 `Z` 开头。在第一个字母之后，结构体名称可以包含下划线 `_`、字母 `a` 到 `z`、字母 `A` 到 `Z` 或数字 `0` 到 `9`。

```move
struct Foo {}
struct BAR {}
struct B_a_z_4_2 {}
```

This naming restriction of starting with `A` to `Z` is in place to give room for future language
features. It may or may not be removed later.

这种以 `A` 到 `Z` 开头的命名限制是为了给未来的语言特性留出空间。以后可能会也可能不会删除这个限制。

## 使用结构体

### 创建结构体

Values of a struct type can be created (or "packed") by indicating the struct name, followed by
value for each field:

可以通过写明结构体名称来创建（或“打包”）结构体类型的值，然后是每个字段的值：

```move
address 0x2 {
module m {
    struct Foo has drop { x: u64, y: bool }
    struct Baz has drop { foo: Foo }

    fun example() {
        let foo = Foo { x: 0, y: false };
        let baz = Baz { foo: foo };
    }
}
}
```

If you initialize a struct field with a local variable whose name is the same as the field, you can
use the following shorthand:

如果使用与字段名相同的局部变量初始化结构体字段，则可以使用以下简写：

```move
let baz = Baz { foo: foo };
// 相当于
let baz = Baz { foo };
```

This is called sometimes called "field name punning".

这有时称为“字段名双关语”。

### 通过模式匹配销毁结构体

Struct values can be destroyed by binding or assigning them patterns.

结构值可以通过绑定或赋值模式来销毁。

```move
address 0x2 {
module m {
    struct Foo { x: u64, y: bool }
    struct Bar { foo: Foo }
    struct Baz {}

    fun example_destroy_foo() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y: foo_y } = foo;
        //        ^ `x: x` 的简写

        // 两个新绑定
        //   x: u64 = 3
        //   foo_y: bool = false
    }

    fun example_destroy_foo_wildcard() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y: _ } = foo;

        // 由于 y 绑定到通配符，因此只有一个新绑定
        //   x: u64 = 3
    }

    fun example_destroy_foo_assignment() {
        let x: u64;
        let y: bool;
        Foo { x, y } = Foo { x: 3, y: false };

        // 改变现有变量 x 和 y
        //   x = 3, y = false
    }

    fun example_foo_ref() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y } = &foo;

        // 两个新绑定
        //   x: &u64
        //   y: &bool
    }

    fun example_foo_ref_mut() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y } = &mut foo;

        // 两个新绑定
        //   x: &mut u64
        //   y: &mut bool
    }

    fun example_destroy_bar() {
        let bar = Bar { foo: Foo { x: 3, y: false } };
        let Bar { foo: Foo { x, y } } = bar;
        //             ^ 嵌套模式

        // 两个新绑定
        //   x: u64 = 3
        //   y: bool = false
    }

    fun example_destroy_baz() {
        let baz = Baz {};
        let Baz {} = baz;
    }
}
}
```

### 借用结构体和字段

The `&` and `&mut` operator can be used to create references to structs or fields. These examples
include some optional type annotations (e.g., `: &Foo`) to demonstrate the type of operations.

`&` 和 `&mut` 运算符可用于创建对结构体或字段的引用。这些例子包括一些可选的类型标注（例如，`: &Foo`）来演示操作的类型。

```move
let foo = Foo { x: 3, y: true };
let foo_ref: &Foo = &foo;
let y: bool = foo_ref.y;          // 通过对结构体的引用读取字段
let x_ref: &u64 = &foo.x;

let x_ref_mut: &mut u64 = &mut foo.x;
*x_ref_mut = 42;            // 通过可变引用修改字段
```

It is possible to borrow inner fields of nested structs.

可以借用嵌套结构体的内部字段：

```move
let foo = Foo { x: 3, y: true };
let bar = Bar { foo };

let x_ref = &bar.foo.x;
```

You can also borrow a field via a reference to a struct.

你还可以通过对结构体的引用来借用字段：

```move
let foo = Foo { x: 3, y: true };
let foo_ref = &foo;
let x_ref = &foo_ref.x;
// 这与 let x_ref = &foo.x 的效果相同
```

### 读写字段

If you need to read and copy a field's value, you can then dereference the borrowed field

如果你需要读取和复制字段的值，则可以解引用借用的字段：

```move
let foo = Foo { x: 3, y: true };
let bar = Bar { foo: copy foo };
let x: u64 = *&foo.x;
let y: bool = *&foo.y;
let foo2: Foo = *&bar.foo;
```

If the field is implicitly copyable, the dot operator can be used to read fields of a struct without
any borrowing. (Only scalar values with the `copy` ability are implicitly copyable.)

如果该字段是隐式可复制的，则点运算符可用于读取结构体的字段而无需任何借用。（只有具有 `copy` 能力的标量值是隐式可复制的。）

```move
let foo = Foo { x: 3, y: true };
let x = foo.x;  // x == 3
let y = foo.y;  // y == true
```

Dot operators can be chained to access nested fields.

点运算符可以链式访问嵌套字段：

```move
let baz = Baz { foo: Foo { x: 3, y: true } };
let x = baz.foo.x; // x = 3;
```

However, this is not permitted for fields that contain non-primitive types, such a vector or another
struct

但是，对于包含非原始类型（例如向量或其他结构体）的字段，这是不允许的：

```move
let foo = Foo { x: 3, y: true };
let bar = Bar { foo };
let foo2: Foo = *&bar.foo;
let foo3: Foo = bar.foo; // 错误！必须使用 *& 添加显式复制
```

The reason behind this design decision is that copying a vector or another struct might be an
expensive operation. It is important for a programmer to be aware of this copy and make others aware
with the explicit syntax `*&`

In addition reading from fields, the dot syntax can be used to modify fields, regardless of the
field being a primitive type or some other struct

这个设计决策背后的原因是复制一个向量或另一个结构体可能是一项昂贵的操作。对于程序员来说，了解这个复制（操作）并使用显式语法 `*&` 让其他人意识到是很重要的。

除了从字段中读取之外，点语法还可用于修改字段，无论该字段是原始类型还是其他结构体。

```move
let foo = Foo { x: 3, y: true };
foo.x = 42;     // foo = Foo { x: 42, y: true }
foo.y = !foo.y; // foo = Foo { x: 42, y: false }
let bar = Bar { foo };            // bar = Bar { foo: Foo { x: 42, y: false } }
bar.foo.x = 52;                   // bar = Bar { foo: Foo { x: 52, y: false } }
bar.foo = Foo { x: 62, y: true }; // bar = Bar { foo: Foo { x: 62, y: true } }
```

The dot syntax also works via a reference to a struct

点语法也适用于对结构体的引用：

```move
let foo = Foo { x: 3, y: true };
let foo_ref = &mut foo;
foo_ref.x = foo_ref.x + 1;
```

## 私有结构体操作

Most struct operations on a struct type `T` can only be performed inside the module that declares
`T`:

- Struct types can only be created ("packed"), destroyed ("unpacked") inside the module that defines
  the struct.
- The fields of a struct are only accessible inside the module that defines the struct.

Following these rules, if you want to modify your struct outside the module, you will need to
provide public APIs for them. The end of the chapter contains some examples of this.

However, struct _types_ are always visible to another module or script:

大多数对结构体类型 `T` 的结构体操作只能在声明 `T` 的模块内执行：

- 结构体类型只能在定义结构体的模块内创建（“打包”）、销毁（“解包”）。
- 结构体的字段只能在定义结构体的模块内部访问。

按照这些规则，如果你想在模块之外修改你的结构体，你需要为他们提供公共 API。本章的最后包含了这方面的一些例子。

但是，结构体类型始终对其他模块或脚本可见：

```move
// m.move
address 0x2 {
module m {
    struct Foo has drop { x: u64 }

    public fun new_foo(): Foo {
        Foo { x: 42 }
    }
}
}
```

```move
// n.move
address 0x2 {
module n {
    use 0x2::m;

    struct Wrapper has drop {
        foo: m::Foo
    }

    fun f1(foo: m::Foo) {
        let x = foo.x;
        //      ^ 错误！此处无法访问 `foo` 的字段
    }

    fun f2() {
        let foo_wrapper = Wrapper { foo: m::new_foo() };
    }
}
}
```

Note that structs do not have visibility modifiers (e.g., `public` or `private`).

请注意，结构体没有可见性修饰符（例如，`public` 或 `private`）。

## 所有权

As mentioned above in [Defining Structs](#defining-structs), structs are by default linear and
ephemeral. This means they cannot be copied or dropped. This property can be very useful when
modeling real world resources like money, as you do not want money to be duplicated or get lost in
circulation.
正如上面[定义结构体](#定义结构体)中提到的，结构体默认是线性的和临时的。这意味着它们不能被复制或删除。在模拟货币等现实世界资源时，此属性非常有用，因为你不希望货币被复制或在流通中丢失。

```move
address 0x2 {
module m {
    struct Foo { x: u64 }

    public fun copying_resource() {
        let foo = Foo { x: 100 };
        let foo_copy = copy foo; // 错误！“复制”需要“复制”能力
        let foo_ref = &foo;
        let another_copy = *foo_ref // 错误！解引用需要“复制”能力
    }

    public fun destroying_resource1() {
        let foo = Foo { x: 100 };

        // 错误！当函数返回时，foo 仍然包含一个值。
        // 这种销毁需要“drop”能力
    }

    public fun destroying_resource2(f: &mut Foo) {
        *f = Foo { x: 100 } // 错误！通过写入销毁旧值需要“drop”能力
    }
}
}
```

To fix the second example (`fun dropping_resource`), you would need to manually "unpack" the resource:

要修复第二个示例（`fun destroying_resource1`），你需要手动“解包”资源：

```move
address 0x2 {
module m {
    struct Foo { x: u64 }

    public fun destroying_resource1_fixed() {
        let foo = Foo { x: 100 };
        let Foo { x: _ } = foo;
    }
}
}
```

Recall that you are only able to deconstruct a resource within the module in which it is defined.
This can be leveraged to enforce certain invariants in a system, for example, conservation of money.

If on the other hand, your struct does not represent something valuable, you can add the abilities
`copy` and `drop` to get a struct value that might feel more familiar from other programming
languages:

回想一下，你只能在定义资源的模块中解构资源。这可以用来在系统中强制执行某些不变量，例如货币守恒。

另一方面，如果你的结构体不代表有价值的东西，你可以添加 `copy` 和 `drop` 能力来获取一个结构值，这感觉可能会与其他编程语言更相似。

```move
address 0x2 {
module m {
    struct Foo has copy, drop { x: u64 }

    public fun run() {
        let foo = Foo { x: 100 };
        let foo_copy = copy foo;
        // ^ 此代码复制 foo，而 `let x = foo` 或
        // `let x = move foo` 都移动 foo

        let x = foo.x;            // x = 100
        let x_copy = foo_copy.x;  // x = 100

        // 函数返回时 foo 和 foo_copy 都被隐式丢弃
    }
}
}
```

## 在全局存储中存储资源

Only structs with the `key` ability can be saved directly in
[persistent global storage](./global-storage-operators.md). All values stored within those `key`
structs must have the `store` abilities. See the [ability](./abilities] and
[global storage](./global-storage-operators.md) chapters for more detail.

只有具有 `key` 能力的结构体才能直接保存在[持久性全局存储](./global-storage-operators.md)中。存储在这些 `key` 结构体中的所有值都必须具有 `store` 能力。有关更多详细信息，请参阅[能力](./abilities.md)和[全局存储](./global-storage-operators.md)章节。

## 示例

Here are two short examples of how you might use structs to represent valuable data (in the case of
`Coin`) or more classical data (in the case of `Point` and `Circle`)

这里有两个简短的示例，说明如何使用结构体来表示有价值的数据（在 `Coin` 的情况下）或更经典的数据（在 `Point` 和 `Circle` 的情况下）。

### 示例 1：Coin

<!-- TODO link to access control for mint -->

```move
address 0x2 {
module m {
    // 我们不希望钱币（Coin）被复制，因为那会复制这个“钱”，
    // 所以我们不会给结构体“copy”能力。
    // 同样，我们不希望程序员销毁钱币，所以我们不会给结构体“drop”能力。
    // 但是，我们*希望*模块的用户能够将这个钱币存储在持久的全局存储中，
    // 所以我们授予结构体“store”能力。
    // 这个结构体只会在全局存储内的其他资源中，因此我们不会赋予该结构体“key”能力。
    struct Coin has store {
        value: u64,
    }

    public fun mint(value: u64): Coin {
        // 你可能希望通过某种形式的访问控制来关闭此（铸币）功能，以防止使用此模块的任何人铸造无限数量的钱币。
        Coin { value }
    }

    public fun withdraw(coin: &mut Coin, amount: u64): Coin {
        assert!(coin.balance >= amount, 1000);
        coin.value = coin.value - amount;
        Coin { value: amount }
    }

    public fun deposit(coin: &mut Coin, other: Coin) {
        let Coin { value } = other;
        coin.value = coin.value + value;
    }

    public fun split(coin: Coin, amount: u64): (Coin, Coin) {
        let other = withdraw(&mut coin, amount);
        (coin, other)
    }

    public fun merge(coin1: Coin, coin2: Coin): Coin {
        deposit(&mut coin1, coin2);
        coin1
    }

    public fun destroy_zero(coin: Coin) {
        let Coin { value } = coin;
        assert!(value == 0, 1001);
    }
}
}
```

### 示例 2：Geometry

```move
address 0x2 {
module point {
    struct Point has copy, drop, store {
        x: u64,
        y: u64,
    }

    public fun new(x: u64, y: u64): Point {
        Point {
            x, y
        }
    }

    public fun x(p: &Point): u64 {
        p.x
    }

    public fun y(p: &Point): u64 {
        p.y
    }

    fun abs_sub(a: u64, b: u64): u64 {
        if (a < b) {
            b - a
        }
        else {
            a - b
        }
    }

    public fun dist_squared(p1: &Point, p2: &Point): u64 {
        let dx = abs_sub(p1.x, p2.x);
        let dy = abs_sub(p1.y, p2.y);
        dx*dx + dy*dy
    }
}
}
```

```move
address 0x2 {
module circle {
    use 0x2::point::{Self, Point};

    struct Circle has copy, drop, store {
        center: Point,
        radius: u64,
    }

    public fun new(center: Point, radius: u64): Circle {
        Circle { center, radius }
    }

    public fun overlaps(c1: &Circle, c2: &Circle): bool {
        let d = point::dist_squared(&c1.center, &c2.center);
        let r1 = c1.radius;
        let r2 = c2.radius;
        d*d <= r1*r1 + 2*r1*r2 + r2*r2
    }
}
}
```
