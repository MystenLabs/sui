# 整数 (Integers)

Move supports three unsigned integer types: `u8`, `u64`, and `u128`. Values of these types range from 0 to a maximum that depends on the size of the type.

| Type                             | Value Range              |
| -------------------------------- | ------------------------ |
| Unsigned 8-bit integer, `u8`     | 0 to 2<sup>8</sup> - 1   |
| Unsigned 64-bit integer, `u64`   | 0 to 2<sup>64</sup> - 1  |
| Unsigned 128-bit integer, `u128` | 0 to 2<sup>128</sup> - 1 |


Move 支持三种无符号整数类型：`u8`、`u64` 和 `u128`。这些类型的值范围从 0 到最大值，最大值的具体取值取决于整数类型。

| 类型                      | 取值范围                 |
|---------------------------|--------------------------|
| 无符号 8位 整数, `u8`     | 0 to 2<sup>8</sup> - 1   |
| 无符号 64位 整数, `u64`   | 0 to 2<sup>64</sup> - 1  |
| 无符号 128位 整数, `u128` | 0 to 2<sup>128</sup> - 1 |


## 字面值(Literal)

Literal values for these types are specified either as a sequence of digits (e.g.,`112`) or as hex literals, e.g., `0xFF`. The type of the literal can optionally be added as a suffix, e.g., `112u8`. If the type is not specified, the compiler will try to infer the type from the context where the literal is used. If the type cannot be inferred, it is assumed to be `u64`.

If a literal is too large for its specified (or inferred) size range, an error is reported.

(在Move中)这些类型的字面值指定为数字序列(例如：112)或十六进制文字(例如：0xFF), 可以选择将字面值的类型定义为后缀, 例如 `112u8`。如果未指定类型，编译器将尝试从使用字面值的上下文推断类型。如果无法推断类型，则默认为 `u64。

如果字面值太大，超出其指定的(或推断的)大小范围，则会报错。

### 例如：

```jsx
// literals with explicit annotations;
let explicit_u8 = 1u8;
let explicit_u64 = 2u64;
let explicit_u128 = 3u128;

// literals with simple inference
let simple_u8: u8 = 1;
let simple_u64: u64 = 2;
let simple_u128: u128 = 3;

// literals with more complex inference
let complex_u8 = 1; // inferred: u8
// right hand argument to shift must be u8
let _unused = 10 << complex_u8;

let x: u8 = 0;
let complex_u8 = 2; // inferred: u8
// arguments to `+` must have the same type
let _unused = x + complex_u8;

let complex_u128 = 3; // inferred: u128
// inferred from function argument type
function_that_takes_u128(complex_u128);

// literals can be written in hex
let hex_u8: u8 = 0x1;
let hex_u64: u64 = 0xCAFE;
let hex_u128: u128 = 0xDEADBEEF;
```

## 运算集 (Operations)

### 算术运算 (Arithmetic)

Each of these types supports the same set of checked arithmetic operations. For all of these operations, both arguments (the left and right side operands) must be of the same type. If you need to operate over values of different types, you will need to first perform a cast. Similarly, if you expect the result of the operation to be too large for the integer type, perform a cast to a larger size before performing the operation.

每一种(无符号整数)类型都支持相同算术运算集。对于所有这些运算，两个参数(左侧和右侧操作数)必须是同一类型。如果您需要对不同类型的值进行运算，则需要首先执行强制转换。同样，如果您预计运算结果对于当下整数类型来说太大，请在执行运算之前将之转换为更大的整数类型。

All arithmetic operations abort instead of behaving in a way that mathematical integers would not (e.g., overflow, underflow, divide-by-zero).

| Syntax | Operation           | Aborts If                                |
| ------ | ------------------- | ---------------------------------------- |
| `+`    | addition            | Result is too large for the integer type |
| `-`    | subtraction         | Result is less than zero                 |
| `*`    | multiplication      | Result is too large for the integer type |
| `%`    | modular division    | The divisor is `0`                       |
| `/`    | truncating division | The divisor is `0`                       |

### [Bitwise](https://move-language.github.io/move/integers.html#bitwise)

算术运算在遇到异常时将会中止，而不是以上溢、下溢、被零除等数学整数未定义的的方式输出结果。

| 句法 | 操作           | 中止条件                                |
| ------ | ------------| ---------------------------------------- |
| `+`    | 加法         | 结果对于整数类型来说太大了 |
| `-`    | 减法         | 结果小于零                 |
| `*`    | 乘法         | 结果对于整数类型来说太大了 |
| `%`    | 取余运算     | 除数为 `0`                       |
| `/`    | 截断除法     | 除数为 `0`                       |

### 位运算 (Bitwise)

The integer types support the following bitwise operations that treat each number as a series of individual bits, either 0 or 1, instead of as numerical integer values.

Bitwise operations do not abort.

| Syntax              | Operation   | Description                                           |
|---------------------|-------------|-------------------------------------------------------|
| `&`                 | bitwise and | Performs a boolean and for each bit pairwise          |
| <code>&#124;</code> | bitwise or  | Performs a boolean or for each bit pairwise           |
| `^`                 | bitwise xor | Performs a boolean exclusive or for each bit pairwise |

整数类型支持下列位运算，即将每个数字视为一系列单独的位：0 或 1，而不是整型数值。

位运算不会中止。

| 语法                | 操作符   | 描述                       |
|---------------------|----------|----------------------------|
| `&`                 | 按位和   | 对每个位成对执行布尔值和   |
| <code>&#124;</code> | 按位或   | 对每个位成对执行布尔值或   |
| `^`                 | 按位异或 | 对每个位成对执行布尔值异或 |

### 位移 (Bit shift)

Similar to the bitwise operations, each integer type supports bit shifts. But unlike the other operations, the righthand side operand (how many bits to shift by) must *always* be a `u8` and need not match the left side operand (the number you are shifting).

Bit shifts can abort if the number of bits to shift by is greater than or equal to `8`, `64`, or `128` for `u8`, `u64`, and `u128` respectively.

| Syntax | Operation   | Aborts if                                                    |
| ------ | ----------- | ------------------------------------------------------------ |
| `<<`   | shift left  | Number of bits to shift by is greater than the size of the integer type |
| `>>`   | shift right | Number of bits to shift by is greater than the size of the integer type |

与按位运算类似，每种整数类型都支持位移(bit shifts)。但与其他运算不同的是，右侧操作数(要移位多少位)必须始终是 `u8`  并且不需要与左侧操作数类型(您要移位的数字)匹配。

如果要移位的位数分别大于或等于 `8`、`64`, `u128` 或 `128` 的 `u8`, `u64`, 则移位可以中止。

| 句法 | 操作   | 中止条件                                                    |
| ------ | ----------- | ------------------------------------------------------------ |
| `<<`   | 左移  | 要移位的位数大于整数类型的大小 |
| `>>`   | 右移 | 要移位的位数大于整数类型的大小 |

### 比较运算 (Comparisons)

Integer types are the *only* types in Move that can use the comparison operators. Both arguments need to be of the same type. If you need to compare integers of different types, you will need to [cast](https://move-language.github.io/move/integers.html#casting) one of them first.

Comparison operations do not abort.

| Syntax | Operation                |
| ------ | ------------------------ |
| `<`    | less than                |
| `>`    | greater than             |
| `<=`   | less than or equal to    |
| `>=`   | greater than or equal to |

整数类型是 Move 中唯一可以使用比较(Comparisons)运算符的类型。两个参数必须是同一类型。如果您需要比较不同类型的整数，则需要先转换其中一个。

比较操作不会中止。

| 句法 | 操作                |
| ------ | ------------------------ |
| `<`    | 小于                |
| `>`    | 大于             |
| `<=`   | 小于等于    |
| `>=`   | 大于等于 |

###  相等 (Equality)

Like all types with [`drop`](https://move-language.github.io/move/abilities.html) in Move, all integer types support the ["equal"](https://move-language.github.io/move/equality.html) and ["not equal"](https://move-language.github.io/move/equality.html) operations. Both arguments need to be of the same type. If you need to compare integers of different types, you will need to [cast](https://move-language.github.io/move/integers.html#casting) one of them first.

Equality operations do not abort.

| Syntax | Operation |
| ------ | --------- |
| `==`   | equal     |
| `!=`   | not equal |

For more details see the section on [equality](https://move-language.github.io/move/equality.html)

与 Move 中的所有具有[`drop`](./chapter_19_abilities.html)能力的类型一样，所有整数类型都支持 ["equal(等于)"](./chapter_11_equality.html) 和 ["not equal(不等于)](./chapter_11_equality.html)运算。两个参数必须是同一类型。如果您需要比较不同类型的整数，则需要先转换其中一个。

相等(Equality)运算不会中止。

| 句法 | 操作 |
| ------ | --------- |
| `==`   | 等于     |
| `!=`   | 不等于 |

更多细节可以参考[相等]([equality](https://move-language.github.io/move/equality.html))章节。

## 转换 (Casting)

Integer types of one size can be cast to integer types of another size. Integers are the only types in Move that support casting.

Casts *do not* truncate. Casting will abort if the result is too large for the specified type

| Syntax     | Operation                                            | Aborts if                              |
| ---------- | ---------------------------------------------------- | -------------------------------------- |
| `(e as T)` | Cast integer expression `e` into an integer type `T` | `e` is too large to represent as a `T` |

Here, the type of `e` must be `u8`, `u64`, or `u128` and `T` must be `u8`, `u64`, or `u128`.

For example:

- `(x as u8)`
- `(2u8 as u64)`
- `(1 + 3 as u128)`

一种大小的整数类型可以转换为另一种大小的整数类型。整数是 Move 中唯一支持强制转换的类型。

强制转换不会截断。如果结果对于指定类型来说太大，则转换将中止。

| Syntax     | 操作                                            | 中止条件                              |
| ---------- | ---------------------------------------------------- | -------------------------------------- |
| `(e as T)` | 将整数表达式 `e` 转换为整数类型 `T` | `e` 太大而不能表示为 `T` |

## 所有权 (Ownership)

As with the other scalar values built-in to the language, integer values are implicitly copyable, meaning they can be copied without an explicit instruction such as [`copy`](https://move-language.github.io/move/variables.html#move-and-copy).

与语言内置的其他标量值一样，整数值是隐式可复制的，这意味着它们可以在没有明确指令如[`copy`](./variables.md#move-and-copy)的情况下复制。
