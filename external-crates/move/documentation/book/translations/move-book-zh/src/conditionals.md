# 条件语句 (Conditionals)

An `if` expression specifies that some code should only be evaluated if a certain condition is true. For example:

`if` 语句可以用来指定一块代码块，但只在判断条件(condition)为true时才会被执行。例如:

```move
if (x > 5) x = x - 5
```

The condition must be an expression of type `bool`.

An `if` expression can optionally include an `else` clause to specify another expression to evaluate when the condition is false.

条件语句(condition)必须是 `bool` 类型的表达式。

`if` 语句可选包含 `else` 子句，以指定当条件(condition)为 false 时要执行的另一个代码块。

```move
if (y <= 10) y = y + 1 else y = 10
```

Either the "true" branch or the "false" branch will be evaluated, but not both. Either branch can be a single expression or an expression block.

The conditional expressions may produce values so that the `if` expression has a result.

无论是"true"分支还是"false"分支都会被执行，但不会同时执行.其中任何一个分支都可以是单行代码或代码块。条件表达式会产生值，所以 `if` 表达式会有一个结果。

```move
let z = if (x < 100) x else 100;
```

The expressions in the true and false branches must have compatible types. For example:

true 和 false 分支的表达式类型必须是一致的,例如:

```move=
// x和y必须是u64整型
// x and y must be u64 integers
let maximum: u64 = if (x > y) x else y;

// 错误！分支的类型不一致
// (ERROR! branches different types)
let z = if (maximum < 10) 10u8 else 100u64;

// 错误！分支的类型不一致,false-branch默认是()不是u64
// ERROR! branches different types, as default false-branch is () not u64
if (maximum >= 10) maximum;
```

If the `else` clause is not specified, the false branch defaults to the unit value. The following are equivalent:

如果`else`子句未定义，false分支默认为 unit 。下面的例子是相等价的:

```move
if (condition) true_branch // implied default: else ()
if (condition) true_branch else ()
```

Commonly, [`if` expressions](./conditionals.md) are used in conjunction with expression blocks.

一般来说, [`if` 表达式](./conditionals.md)与多个表达式块结合使用.

```move
let maximum = if (x > y) x else y;
if (maximum < 10) {
    x = x + 10;
    y = y + 10;
} else if (x >= 10 && y >= 10) {
    x = x - 10;
    y = y - 10;
}
```

## 条件语句的语法 (Grammar for Conditionals)

> *if-expression* → **if (** *expression* **)** *expression* *else-clause*<sub>*opt*</sub>
> *else-clause* → **else** *expression*
