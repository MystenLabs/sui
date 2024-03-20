# 布尔类型 (Bool)

`bool`is Move's primitive type for boolean `true` and `false`values.

`bool` 是 Move 布尔基本类型，有 `true` 和 `false` 两个值。

## 字面量 (Literals)

Literals for `bool` are either `true` or `false` .

布尔类型字面值只能是 `true` 或者 `false`中的一个 。

## 操作 (Operations)

### 逻辑运算 (Logical)

`bool`supports three logical operations:

| Syntax                    | Description                  | Equivalent Expression                                               |
| ------------------------- | ---------------------------- | ------------------------------------------------------------------- |
| `&&`                      | short-circuiting logical and | `p && q` is equivalent to `if (p) q else false`                     |
| <code>&vert;&vert;</code> | short-circuiting logical or  | <code>p &vert;&vert; q</code> is equivalent to `if (p) true else q` |
| `!`                       | logical negation             | `!p` is equivalent to `if (p) false else true`                      |


`bool` 支持三种逻辑运算：

| 句法 | 描述                  | Equivalent Expression                           |
| ------ | ---------------------------- | ----------------------------------------------- |
| `&&`   | 短路逻辑与(short-circuiting logical and) | `p && q` 等价于 `if (p) q else false` |
| <code>&vert;&vert;</code>   | 短路逻辑或(short-circuiting logical or)  | `p || q` 等价于 `if (p) true else q`  |
| `!`    | 逻辑非(logical negation)            | `!p` 等价于 `if (p) false else true`  |


### 控制流 (Control Flow)

`bool`values are used in several of Move's control-flow constructs:

布尔值用于 Move 的多个控制流结构中：

- [`if (bool) { ... }`](./conditionals.html)
- [`while (bool) { .. }`](/loops.html)
- [`assert!(bool, u64)`](./abort-and-assert.html)

## 所有权 (Ownership)

As with the other scalar values built-in to the language, boolean values are implicitly copyable, meaning they can be copied without an explicit instruction such as `[copy](<https://move-language.github.io/move/variables.html#move-and-copy>).`

与语言内置的其他标量值一样，布尔值是隐式可复制的，这意味着它们可以在没有明确指令如[`copy`](./variables.md#move-and-copy)的情况下复制。
