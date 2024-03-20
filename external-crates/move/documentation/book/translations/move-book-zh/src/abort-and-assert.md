# 中止和断言 (Abort and Assert)

[`return`](./functions.md) and `abort` are two control flow constructs that end execution, one for
the current function and one for the entire transaction.

More information on [`return` can be found in the linked section](./functions.md)

[`return`](./functions.md) 和 `abort` 是两种结束程序执行的控制流结构。前者针对当前函数，后者针对整个事务。

 [`return`](./functions.md)的更多信息可以参考链接中的文章。

## `abort` 中止

`abort` is an expression that takes one argument: an **abort code** of type `u64`. For example:

`abort` 表达式只接受一个参数: 类型为 `u64` 的**中止代码**。例如：

```move
abort 42
```

The `abort` expression halts execution the current function and reverts all changes made to global
state by the current transaction. There is no mechanism for "catching" or otherwise handling an `abort`.

`abort` 表达式会中止执行当前函数并恢复当前事务对全局状态所做的所有更改。Move语言没有“捉捕”或者额外处理`abort`的机制。

Luckily, in Move transactions are all or nothing, meaning any changes to global storage are made all
at once only if the transaction succeeds. Because of this transactional commitment of changes, after
an abort there is no need to worry about backing out changes. While this approach is lacking in
flexibility, it is incredibly simple and predictable.

幸运的是，在Move里事务的计算要么完全执行要么完全不执行。这意味着只有在事务成功时，任何对全局存储状态的改变才会被一并执行。
由于这种对于所有更改的事务承诺，在 `abort` 之后我们不需要担心去回滚任何更改。尽管这种方法缺少灵活性，它还是非常简单和可预测的。


Similar to [`return`](./functions.md), `abort` is useful for exiting control flow when some condition cannot be met.

In this example, the function will pop two items off of the vector, but will abort early if the vector does not have two items

与 [`return`](./functions.md)相似, 在一些条件无法被满足的时候，`abort` 可以被用于退出控制流(control flow)。

在以下示例中，目标函数会从vector里弹出两个元素，但是如果vector中并没有两个元素，函数会提前中止。

```move=
use std::vector;
fun pop_twice<T>(v: &mut vector<T>): (T, T) {
    if (vector::length(v) < 2) abort 42;

    (vector::pop_back(v), vector::pop_back(v))
}
```

This is even more useful deep inside a control-flow construct. For example, this function checks
that all numbers in the vector are less than the specified `bound`. And aborts otherwise

这在控制流结构的深处甚至会更有用。例如，此函数检查vector中是否所有数字都小于指定的边界(`bound`)。否则函数中止：

```move=
use std::vector;
fun check_vec(v: &vector<u64>, bound: u64) {
    let i = 0;
    let n = vector::length(v);
    while (i < n) {
        let cur = *vector::borrow(v, i);
        if (cur > bound) abort 42;
        i = i + 1;
    }
}
```

### `assert` 断言

`assert` is a builtin, macro-like operation provided by the Move compiler. It takes two arguments, a condition of type `bool` and a code of type `u64`

`assert` 是 Move 编译器提供的内置的类宏(macro-like)操作。它需要两个参数：一个 `bool` 类型的条件和一个 `u64` 类型的错误状态码(类似HTTP中的StatusCode: 404, 500等，译者注)

```move
assert!(condition: bool, code: u64)
```

Since the operation is a macro, it must be invoked with the `!`. This is to convey that the
arguments to `assert` are call-by-expression. In other words, `assert` is not a normal function and
does not exist at the bytecode level. It is replaced inside the compiler with

由于该操作是一个宏，因此必须使用 `!` 调用它。这是为了表达 `assert` 的参数属于表达式调用(call-by-expression)。换句话说，`assert` 不是一个正常的函数，并且在字节码(bytecode)级别不存在。它在编译器内部被替换为以下代码：

```move
if (condition) () else abort code
```

`assert` is more commonly used than just `abort` by itself. The `abort` examples above can be rewritten using `assert`

`assert` 比 `abort` 本身更常用。上面的 `abort` 示例可以使用 `assert` 重写

```move=
use std::vector;
fun pop_twice<T>(v: &mut vector<T>): (T, T) {
    assert!(vector::length(v) >= 2, 42); // 现在使用'assert'

    (vector::pop_back(v), vector::pop_back(v))
}
```

和

```move=
use std::vector;
fun check_vec(v: &vector<u64>, bound: u64) {
    let i = 0;
    let n = vector::length(v);
    while (i < n) {
        let cur = *vector::borrow(v, i);
        assert!(cur <= bound, 42); // 现在使用 'assert'
        i = i + 1;
    }
}
```

Note that because the operation is replaced with this `if-else`, the argument for the `code` is not
always evaluated. For example:

请注意，因为此操作被替换为 `if-else`，这段 `代码` 的参数不是总是被执行(evaluated)。例如：

```move
assert!(true, 1 / 0)
```

Will not result in an arithmetic error, it is equivalent to

不会导致算术错误，因为它相当于：

```move
if (true) () else (1 / 0)
```

So the arithmetic expression is never evaluated!

所以这个算术表达式永远不会被执行(evaluated)！

### Abort codes in the Move VM (Move虚拟机中的中止代码)

When using `abort`, it is important to understand how the `u64` code will be used by the VM.

Normally, after successful execution, the Move VM produces a change-set for the changes made to
global storage (added/removed resources, updates to existing resources, etc).

当使用 `abort` 时，理解虚拟机将如何使用 `u64` 代码是非常重要的。

通常，在成功执行后，Move 虚拟机会为对全局存储(添加/删除资源、更新现有资源等)所做的更改生成一个更改集。

If an `abort` is reached, the VM will instead indicate an error. Included in that error will be two
pieces of information:

- The module that produced the abort (address and name)
- The abort code.

For example

如果执行到 `abort` 代码，虚拟机将指示错误。该错误中包含两块信息：

- 发生中止的模块(地址和名称)
- 错误状态码。

例如

```move=
address 0x2 {
    module example {
        public fun aborts() {
            abort 42
        }
    }
}

script {
    fun always_aborts() {
        0x2::example::aborts()
    }
}
```

If a transaction, such as the script `always_aborts` above, calls `0x2::example::aborts`, the VM
would produce an error that indicated the module `0x2::example` and the code `42`.

This can be useful for having multiple aborts being grouped together inside a module.

In this example, the module has two separate error codes used in multiple functions

如果一个事务，例如上面的脚本 `always_aborts` 调用了 `0x2::example::aborts`，虚拟机将产生一个指示模块 `0x2::example` 和错误状态码 `42` 的错误。

这在一个模块内将多个中止功能组合起来会很有用。

在以下示例中，模块有两个单独的错误状态码，用于多个函数

```move=
address 0x42 {
    module example {

        use std::vector;

        const EMPTY_VECTOR: u64 = 0;
        const INDEX_OUT_OF_BOUNDS: u64 = 1;

        // 移动 i 到 j, 移动 j 到 k, 移动 k 到 i
        public fun rotate_three<T>(v: &mut vector<T>, i: u64, j: u64, k: u64) {
            let n = vector::length(v);
            assert!(n > 0, EMPTY_VECTOR);
            assert!(i < n, INDEX_OUT_OF_BOUNDS);
            assert!(j < n, INDEX_OUT_OF_BOUNDS);
            assert!(k < n, INDEX_OUT_OF_BOUNDS);

            vector::swap(v, i, k);
            vector::swap(v, j, k);
        }

        public fun remove_twice<T>(v: &mut vector<T>, i: u64, j: u64): (T, T) {
            let n = vector::length(v);
            assert!(n > 0, EMPTY_VECTOR);
            assert!(i < n, INDEX_OUT_OF_BOUNDS);
            assert!(j < n, INDEX_OUT_OF_BOUNDS);
            assert!(i > j, INDEX_OUT_OF_BOUNDS);

            (vector::remove<T>(v, i), vector::remove<T>(v, j))
        }
    }
}
```

## The type of `abort` (`abort` 的类型)

The `abort i` expression can have any type! This is because both constructs break from the normal
control flow, so they never need to evaluate to the value of that type.

The following are not useful, but they will type check

`abort i` 表达式可以有任何类型！这是因为这两种构造都打破了正常控制流，因此他们永远不需要计算该类型的值。

以下的示例不是特别有用，但它们会做类型检查

```move
let y: address = abort 0;
```

This behavior can be helpful in situations where you have a branching instruction that produces a
value on some branches, but not all. For example:

在您有一个分支指令，并且这个指令会产生某些分支(不是全部)的值的时候，这种行为会非常有用。例如：

```move
let b =
    if (x == 0) false
    else if (x == 1) true
    else abort 42;
//       ^^^^^^^^ `abort 42` 的类型是 `bool`
```
