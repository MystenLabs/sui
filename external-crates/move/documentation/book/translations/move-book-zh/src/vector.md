# 向量

`vector<T>` 是 Move 提供的唯一原始集合类型。`vector<T>` 是类型为 `T` 的同构集合，可以通过从"末端"推入/弹出（出栈/入栈，译者注）值来增长或缩小。
*（与 Rust 一样，向量（vector）是一种可以存放任何类型的可变大小的容器，也可称为[动态数组](https://en.wikipedia.org/wiki/Dynamic_array)，与 Python 中的[列表（list）](https://computersciencewiki.org/index.php/Lists)不同，译者注）*

`vector<T>` 可以用任何类型 `T` 实例化。例如，`vector<u64>`、`vector<address>`、`vector<0x42::MyModuel::MyResource>` 和 `vector<vector<u8>>` 都是有效的向量类型。

## 字面量

### 通用 `vector` 字面量

任何类型的向量都可以通过 `vector` 字面量创建。

| 语法                  | 类型                                                                          | 描述                              |
|-----------------------|-------------------------------------------------------------------------------|-----------------------------------|
| `vector[]`            | `vector[]: vector<T>` 其中 `T` 是任何单一的非引用类型                         | 一个空向量                        |
| `vector[e1, ..., en]` | `vector[e1, ..., en]: vector<T>` where `e_i: T` 满足 `0 < i <= n` and `n > 0` | 带有 `n` 个元素（长度为 n）的向量 |

在这些情况下，`vector` 的类型是从元素类型或从向量的使用上推断出来的。如果无法推断类型或者只是为了更清楚地表示，则可以显式指定类型：

```move
vector<T>[]: vector<T>
vector<T>[e1, ..., en]: vector<T>
```

#### 向量字面量示例

```move
(vector[]: vector<bool>);
(vector[0u8, 1u8, 2u8]: vector<u8>);
(vector<u128>[]: vector<u128>);
(vector<address>[@0x42, @0x100]: vector<address>);
```

### `vector<u8>` 字面量

Move 中向量的一个常见用例是表示“字节数组”，用 `vector<u8>` 表示。这些值通常用于加密目的，例如公钥或哈希结果。这些值非常常见，以至于提供了特定的语法以使值更具可读性，而不是必须使用 `vector[]`，其中每个单独的 `u8` 值都以数字形式指定。

目前支持两种类型的 `vector<u8>` 字面量，*字节字符串*和*十六进制字符串*。

#### 字节字符串

字节字符串是带引号的字符串字面量，以 `b` 为前缀，例如，`b"Hello!\n"`。

这些是允许转义序列的 ASCII 编码字符串。目前，支持的转义序列如下：

| 转义序列 | 描述                                        |
|----------|---------------------------------------------|
| `\n`     | 换行                                        |
| `\r`     | 回车                                        |
| `\t`     | 制表符                                      |
| `\\`     | 反斜杠                                      |
| `\0`     | Null                                        |
| `\"`     | 引号                                        |
| `\xHH`   | 十六进制进制转义，插入十六进制字节序列 `HH` |

#### 十六进制字符串

十六进制字符串是以 `x` 为前缀的带引号的字符串字面量，例如，`x"48656C6C6F210A"`。

每个字节对，范围从 `00` 到 `FF` 都被解析为十六进制编码的 `u8` 值。所以每个字节对对应于结果 `vector<u8>` 的单个条目。

#### 字符串字面量示例

```move
script {
    fun byte_and_hex_strings() {
        assert!(b"" == x"", 0);
        assert!(b"Hello!\n" == x"48656C6C6F210A", 1);
        assert!(b"\x48\x65\x6C\x6C\x6F\x21\x0A" == x"48656C6C6F210A", 2);
        assert!(
            b"\"Hello\tworld!\"\n \r \\Null=\0" ==
                x"2248656C6C6F09776F726C6421220A200D205C4E756C6C3D00",
            3
        );
    }
}
```

## 操作

`vector` 通过 Move 标准库里的 `std::vector` 模块支持以下操作：

| 函数                                                       | 描述                                                                                                         | 中止条件             |
|------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|----------------------|
| `vector::empty<T>(): vector<T>`                            | 创建一个可以存储 `T` 类型值的空向量                                                                          | 永不中止             |
| `vector::singleton<T>(t: T): vector<T>`                    | 创建一个包含 `t` 的大小为 1 的向量                                                                           | 永不中止             |
| `vector::push_back<T>(v: &mut vector<T>, t: T)`            | 将 `t` 添加到 `v` 的尾部                                                                                     | 永不中止             |
| `vector::pop_back<T>(v: &mut vector<T>): T`                | 移除并返回 `v` 中的最后一个元素                                                                              | 如果 `v` 是空向量    |
| `vector::borrow<T>(v: &vector<T>, i: u64): &T`             | 返回在索引 `i` 处对 `T` 的不可变引用                                                                         | 如果 `i` 越界        |
| `vector::borrow_mut<T>(v: &mut vector<T>, i: u64): &mut T` | 返回在索引 `i` 处对 `T` 的可变引用                                                                           | 如果 `i` 越界        |
| `vector::destroy_empty<T>(v: vector<T>)`                   | 销毁 `v` 向量                                                                                                | 如果 `v` 不是空向量  |
| `vector::append<T>(v1: &mut vector<T>, v2: vector<T>)`     | 将 `v2` 中的元素添加到 `v1` 的末尾                                                                           | 永不中止             |
| `vector::contains<T>(v: &vector<T>, e: &T): bool`          | 如果 `e` 在向量 `v` 里返回 true，否则返回 false                                                              | 永不中止             |
| `vector::swap<T>(v: &mut vector<T>, i: u64, j: u64)`       | 交换向量 `v` 中第 `i` 个和第 `j` 个索引处的元素                                                              | 如果 `i` 或 `j` 越界 |
| `vector::reverse<T>(v: &mut vector<T>)`                    | 反转向量 `v` 中元素的顺序                                                                                    | 永不中止             |
| `vector::index_of<T>(v: &vector<T>, e: &T): (bool, u64)`   | 如果 `e` 在索引 `i` 处的向量中，则返回 `(true, i)`。否则返回`(false, 0)`                                     | 永不中止             |
| `vector::remove<T>(v: &mut vector<T>, i: u64): T`          | 移除向量 `v` 中的第 `i` 个元素，移动所有后续元素。这里的时间复杂度是 O(n)，并且保留了向量中元素的顺序        | 如果 `i` 越界        |
| `vector::swap_remove<T>(v: &mut vector<T>, i: u64): T`     | 将向量中的第 `i` 个元素与最后一个元素交换，然后弹出该元素。这里的时间复杂度是 O(1)，但是不保留向量中的元素顺序 | 如果 `i` 越界        |

随着时间的推移可能会增加更多操作。

## 示例

```move
use std::vector;

let v = vector::empty<u64>();
vector::push_back(&mut v, 5);
vector::push_back(&mut v, 6);

assert!(*vector::borrow(&v, 0) == 5, 42);
assert!(*vector::borrow(&v, 1) == 6, 42);
assert!(vector::pop_back(&mut v) == 6, 42);
assert!(vector::pop_back(&mut v) == 5, 42);
```

## 销毁和复制 `vector`

`vector<T>` 的某些行为取决于元素类型 `T` 的能力（ability），例如：如果向量中包含不具有 `drop` 能力的元素，那么不能像上面例子中的 `v` 一样隐式丢弃 —— 它们必须用 `vector::destroy_empty` 显式销毁。

请注意，除非向量 `vec` 包含零个元素，否则 `vector::destroy_empty` 将在运行时中止：

```move
fun destroy_any_vector<T>(vec: vector<T>) {
    vector::destroy_empty(vec) // 删除此行将导致编译器错误
}
```

但是删除包含带有 `drop` 能力的元素的向量不会发生错误：

```move
fun destroy_droppable_vector<T: drop>(vec: vector<T>) {
    // 有效！
    // 不需要明确地做任何事情来销毁向量
}
```

同样，除非元素类型具有 `copy` 能力，否则无法复制向量。换句话说，当且仅当 `T` 具有 `copy` 能力时，`vector<T>` 才具有 `copy` 能力。然而，即使是可复制的向量也永远不会被隐式复制：

```move
let x = vector::singleton<u64>(10);
let y = copy x; // 没有 copy 将导致编译器错误！
```

大向量的复制可能很昂贵，因此编译器需要显式 `copy` 以便更容易查看它们发生的位置。

有关更多详细信息，请参阅[类型能力](./abilities.md)和[泛型](./generics.md)部分。

## 所有权

[如上所述](#销毁和复制-vector)，`vector` 值只有在元素值可以复制的时候才能复制。在这种情况下，复制必须通过显式 [`copy`](./variables.md#移动和复制) 或者[解引用 `*`](./references.md#通过引用读取和写入)。
