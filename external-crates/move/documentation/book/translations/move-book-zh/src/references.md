# 引用(references)

Move has two types of references: immutable `&` and mutable `&mut`. Immutable references are read
only, and cannot modify the underlying value (or any of its fields). Mutable references allow for
modifications via a write through that reference. Move's type system enforces an ownership
discipline that prevents reference errors.

Move 支持两种类型的引用：不可变引用 `&` 和可变引用 `&mut`。不可变引用是只读的，不能修改相关值(或其任何字段)。可变引用通过写入该引用进行修改。Move的类型系统强制执行所有权规则，以避免引用错误。

For more details on the rules of references, see [Structs and Resources](./structs-and-resources.md)

更多有关引用规则的详细信息，请参阅：[结构和资源](./structs-and-resources.html).

## 引用运算符 (Reference Operators)

Move provides operators for creating and extending references as well as converting a mutable
reference to an immutable one. Here and elsewhere, we use the notation `e: T` for "expression `e`
has type `T`".

Move 提供了用于创建和扩展引用以及将可变引用转换为不可变引用的运算符。在这里和其他地方，我们使用符号 `e: T` 来表示“表达式 `e` 的类型是 `T` ”

| Syntax      | Type                                                  | Description                                                    |
| ----------- | ----------------------------------------------------- | -------------------------------------------------------------- |
| `&e`        | `&T` where `e: T` and `T` is a non-reference type     | Create an immutable reference to `e`                           |
| `&mut e`    | `&mut T` where `e: T` and `T` is a non-reference type | Create a mutable reference to `e`.                             |
| `&e.f`      | `&T` where `e.f: T`                                   | Create an immutable reference to field `f` of struct `e`.      |
| `&mut e.f`  | `&mut T` where `e.f: T`                               | Create a mutable reference to field `f` of struct`e`.          |
| `freeze(e)` | `&T` where `e: &mut T`                                | Convert the mutable reference `e` into an immutable reference. |

| 语法 | 类型 | 描述 |
| ------      | ------ |------ |
| `&e`        | `&T` 其中 `e: T` 和 `T` 是非引用类型      | 创建一个不可变的引用 `e`
| `&mut e`    | `&mut T` 其中 `e: T` 和 `T` 是非引用类型  | 创建一个可变的引用 `e`
| `&e.f`      | `&T` 其中 `e.f: T`                       | 创建结构 `e` 的字段 `f` 的不可变引用
| `&mut e.f`  | `&mut T` 其中`e.f: T`                    | 创建结构 `e` 的字段 `f` 的可变引用
| `freeze(e)` | `&T` 其中`e: &mut T`                     | 将可变引用 `e` 转换为不可变引用

The `&e.f` and `&mut e.f` operators can be used both to create a new reference into a struct or to extend an existing reference:

`&e.f`和`&mut e.f`运算符既可以用于在结构中创建新引用，也可以用于扩展现有引用：

```move
let s = S { f: 10 };
let f_ref1: &u64 = &s.f; // works
let s_ref: &S = &s;
let f_ref2: &u64 = &s_ref.f // also works
```

A reference expression with multiple fields works as long as both structs are in the same module:

只要两个结构都在同一个模块中，具有多个字段的引用表达式就可以工作：

```move
struct A { b: B }
struct B { c : u64 }
fun f(a: &A): &u64 {
  &a.b.c
}
```

Finally, note that references to references are not allowed:

最后，请注意，不允许引用"引用"(Move不支持多重引用, 但Rust可以，译者注)：

```move
let x = 7;
let y: &u64 = &x;
let z: &&u64 = &y; // will not compile
```

## 通过引用读取和写入

Both mutable and immutable references can be read to produce a copy of the referenced value.

Only mutable references can be written. A write `*x = v` discards the value previously stored in `x`
and updates it with `v`.

可以读取可变和不可变引用以生成引用值的副本。

只能写入可变引用。写入表达式 `*x = v` 会丢弃先前存储在x中的值，并用 `v` 更新。

Both operations use the C-like `*` syntax. However, note that a read is an expression, whereas a
write is a mutation that must occur on the left hand side of an equals.

两种操作都使用类 C `*` 语法。但是请注意，读取是一个表达式，而写入是一个必须发生在等号左侧的改动。

| Syntax     | Type                                | Description                         |
| ---------- | ----------------------------------- | ----------------------------------- |
| `*e`       | `T` where `e` is `&T` or `&mut T`   | Read the value pointed to by `e`    |
| `*e1 = e2` | `()` where `e1: &mut T` and `e2: T` | Update the value in `e1` with `e2`. |

| 语法 | 类型 | 描述 |
| ------ | ------ |------ |
| `*e` | `T` 其中 `e` 为 `&T` 或 `&mut T` | 读取 `e` 所指向的值
| `*e1 = e2` | () 其中 `e1: &mut T` 和 `e2: T` | 用 `e2` 更新 `e1` 中的值

In order for a reference to be read, the underlying type must have the
[`copy` ability](./abilities.md) as reading the reference creates a new copy of the value. This rule
prevents the copying of resource values:

为了读取引用，相关类型必须具备[`copy` 能力](./abilities.html)，因为读取引用会创建值的新副本。此规则防止复制资源值：

```move=
fun copy_resource_via_ref_bad(c: Coin) {
    let c_ref = &c;
    let counterfeit: Coin = *c_ref; // not allowed!
    pay(c);
    pay(counterfeit);
}
```

Dually: in order for a reference to be written to, the underlying type must have the
[`drop` ability](./abilities.md) as writing to the reference will discard (or "drop") the old value.
This rule prevents the destruction of resource values:

双重性：为了写入引用，相关类型必须具备[`drop` 能力](./abilities.html)，因为写入引用将丢弃(或“删除”)旧值。此规则可防止破坏资源值：

```move=
fun destroy_resource_via_ref_bad(ten_coins: Coin, c: Coin) {
    let ref = &mut ten_coins;
    *ref = c; // not allowed--would destroy 10 coins!
}
```

## `freeze` 推断 (`freeze` inference)

A mutable reference can be used in a context where an immutable reference is expected:

可变引用可以在预期不可变引用的上下文中使用：

```move
let x = 7;
let y: &mut u64 = &mut x;
```

This works because the under the hood, the compiler inserts `freeze` instructions where they are
needed. Here are a few more examples of `freeze` inference in action:

这是因为编译器会在底层需要的地方插入 `freeze` 指令。以下是更多 `freeze` 实际推断行为的示例：

```move=
fun takes_immut_returns_immut(x: &u64): &u64 { x }

// freeze inference on return value
fun takes_mut_returns_immut(x: &mut u64): &u64 { x }

fun expression_examples() {
    let x = 0;
    let y = 0;
    takes_immut_returns_immut(&x); // no inference
    takes_immut_returns_immut(&mut x); // inferred freeze(&mut x)
    takes_mut_returns_immut(&mut x); // no inference

    assert!(&x == &mut y, 42); // inferred freeze(&mut y)
}

fun assignment_examples() {
    let x = 0;
    let y = 0;
    let imm_ref: &u64 = &x;

    imm_ref = &x; // no inference
    imm_ref = &mut y; // inferred freeze(&mut y)
}
```

###  子类型化 (Subtyping)

With this `freeze` inference, the Move type checker can view `&mut T` as a subtype of `&T`. As shown
above, this means that anywhere for any expression where a `&T` value is used, a `&mut T` value can
also be used. This terminology is used in error messages to concisely indicate that a `&mut T` was
needed where a `&T` was supplied. For example

通过freeze推断，Move 类型检查器可以将 `&mut T` 视为 `&T` 的子类型。 如上所示，这意味着对于使用 `&T` 值的任何表达式，也可以使用 `&mut T` 值。此术语用于错误消息中，以简明扼要地表明在提供 `&T` 的地方需要 `&mut T` 。例如:

```move=
address 0x42 {
    module example {
        fun read_and_assign(store: &mut u64, new_value: &u64) {
            *store = *new_value
        }

        fun subtype_examples() {
            let x: &u64 = &0;
            let y: &mut u64 = &mut 1;

            x = &mut 1; // valid
            y = &2; // invalid!

            read_and_assign(y, x); // valid
            read_and_assign(x, y); // invalid!
        }
    }
}
```

will yield the following error messages

将产生以下错误消息

```text
error:
    ┌── example.move:12:9 ───
    │
 12 │         y = &2; // invalid!
    │         ^ Invalid assignment to local 'y'
    ·
 12 │         y = &2; // invalid!
    │             -- The type: '&{integer}'
    ·
  9 │         let y: &mut u64 = &mut 1;
    │                -------- Is not a subtype of: '&mut u64'
    │

error:
    ┌── example.move:15:9 ───
    │
 15 │         read_and_assign(x, y); // invalid!
    │         ^^^^^^^^^^^^^^^^^^^^^ Invalid call of '0x42::example::read_and_assign'. Invalid argument for parameter 'store'
    ·
  8 │         let x: &u64 = &0;
    │                ---- The type: '&u64'
    ·
  3 │     fun read_and_assign(store: &mut u64, new_value: &u64) {
    │                                -------- Is not a subtype of: '&mut u64'
    │
```

The only other types currently that has subtyping are [tuples](./tuples.md)

当前唯一具有子类型的其他类型是[tuple(元组)](./tuples.html)

## 所有权 (Ownership)

Both mutable and immutable references can always be copied and extended _even if there are existing
copies or extensions of the same reference_:

_即使同一引用存在现有副本或扩展_，可变引用和不可变引用始终可以被复制和扩展：

```move
fun reference_copies(s: &mut S) {
  let s_copy1 = s; // ok
  let s_extension = &mut s.f; // also ok
  let s_copy2 = s; // still ok
  ...
}
```

This might be surprising for programmers familiar with Rust's ownership system, which would reject
the code above. Move's type system is more permissive in its treatment of
[copies](./variables.md#move-and-copy), but equally strict in ensuring unique ownership of mutable
references before writes.

对于熟悉 Rust 所有权系统的程序员来说，这可能会令人惊讶，因为他们会拒绝上面的代码。Move 的类型系统在处理[副本](./variables.html#move-and-copy)方面更加宽松 ，但在写入前确保可变引用的唯一所有权方面同样严格。

### 无法存储引用 (References Cannot Be Stored)

References and tuples are the _only_ types that cannot be stored as a field value of structs, which
also means that they cannot exist in global storage. All references created during program execution
will be destroyed when a Move program terminates; they are entirely ephemeral. This invariant is
also true for values of types without the `store` [ability](./abilities.md), but note that
references and tuples go a step further by never being allowed in structs in the first place.

This is another difference between Move and Rust, which allows references to be stored inside of
structs.

引用和元组是唯一不能存储为结构的字段值的类型，这也意味着它们不能存在于全局存储中。当 Move 程序终止时，程序执行期间创建的所有引用都将被销毁；它们完全是短暂的。这种不变式也适用于没有[`store` 能力](./chatper_19_abilities.html)的类型的值，但请注意，引用和元组更进一步，从一开始就不允许出现在结构中。

这是 Move 和 Rust 之间的另一个区别，后者允许将引用存储在结构内。

Currently, Move cannot support this because references cannot be
[serialized](https://en.wikipedia.org/wiki/Serialization), but _every Move value must be
serializable_. This requirement comes from Move's
[persistent global storage](./global-storage-structure.md), which needs to serialize values to
persist them across program executions. Structs can be written to global storage, and thus they must
be serializable.

One could imagine a fancier, more expressive, type system that would allow references to be stored
in structs _and_ ban those structs from existing in global storage. We could perhaps allow
references inside of structs that do not have the `store` [ability](./abilities.md), but that would
not completely solve the problem: Move has a fairly complex system for tracking static reference
safety, and this aspect of the type system would also have to be extended to support storing
references inside of structs. In short, Move's type system (particularly the aspects around
reference safety) would have to expand to support stored references. But it is something we are
keeping an eye on as the language evolves.

目前，Move 无法支持这一点，因为引用无法被[序列化](https://en.wikipedia.org/wiki/Serialization)，但 _每个 Move 值都必须是可序列化的_。这个要求来自于 Move 的 [持久化全局存储](./global-storage-structure.html)，它需要在程序执行期间序列化值以持久化它们。结构体可以写入全局存储，因此它们必须是可序列化的。

可以想象一种更奇特、更有表现力的类型系统，它允许将引用存储在结构中，并禁止这些结构存在于全局存储中。我们也许可以允许在没有[`store` 能力](./abilities.html)的结构内部使用引用，但这并不能完全解决问题：Move 有一个相当复杂的系统来跟踪静态引用安全性，并且类型系统的这一方面也必须扩展以支持在结构内部存储引用。简而言之，Move 的类型系统(尤其是与引用安全相关的方面)需要扩展以支持存储的引用。随着语言的发展，我们正在关注这一点。
