# 泛型

泛型可用于定义具有不同输入数据类型的函数和结构体。这种语言特性有时被称为*参数多态*（parametric polymorphism）。在 Move 中，我们经常将术语泛型与类型形参（type parameter）和类型实参（type argument）互换使用。*（有些书籍的中文翻译通常将 type parameter 和 type argument 不加以区别地翻译为“类型参数”，译者注）*

泛型通常用于库（library）代码中，例如向量中，声明适用于任何可能的实例化（满足指定约束）的代码。在其他框架中，泛型代码有时可用多种不同的方式与全局存储进行交互，这些方式有着相同的实现。

## 声明类型参数

函数和结构体都可以在其签名中带上类型参数列表，由一对尖括号括起来 `<...>`。

### 泛型函数

函数的类型参数放在函数名称之后和（值）参数列表之前。以下代码定义了一个泛型标识函数，该函数接受任何类型的值并返回原值。

```move
fun id<T>(x: T): T {
    // 此类型标注是不必要但有效的
    (x: T)
}
```

一旦定义，类型参数 `T` 就可以在参数类型、返回类型和函数体内使用。

### 泛型结构体

结构体的类型参数放在结构名称之后，可用于命名字段的类型。

```move
struct Foo<T> has copy, drop { x: T }

struct Bar<T1, T2> has copy, drop {
    x: T1,
    y: vector<T2>,
}
```

请注意，[未使用的类型参数](#未使用的类型参数)。

## 类型实参

### 调用泛型函数

调用泛型函数时，可以在由一对尖括号括起来的列表中为函数的类型形参指定类型实参。

```move
fun foo() {
    let x = id<bool>(true);
}
```

如果你不指定类型实参，Move 的[类型推断](#类型推断)（功能）将为你提供它们。

### 使用泛型结构体

类似地，在构造或销毁泛型类型的值时，可以为结构体的类型参数附加一个类型实参列表。

```move
fun foo() {
    let foo = Foo<bool> { x: true };
    let Foo<bool> { x } = foo;
}
```

如果你不指定类型实参，Move 的[类型推断](#类型推断)（功能）将为你提供它们。

### 类型实参不匹配

如果你指定类型实参并且它们与提供的实际值冲突，则会报错：

```move
fun foo() {
    let x = id<u64>(true); // 错误！true 不是 u64
}
```

同样地：

```move
fun foo() {
    let foo = Foo<bool> { x: 0 }; // 错误！0 不是布尔值
    let Foo<address> { x } = foo; // 错误！bool 与 address 不兼容
}
```

## 类型推断

在大多数情况下，Move 编译器能够推断类型实参，因此你不必显式地写下它们。如果我们省略类型实参，上面的例子会是这样的：

```move
fun foo() {
    let x = id(true);
    //        ^ 被推断为 <bool>

    let foo = Foo { x: true };
    //           ^ 被推断为 <bool>

    let Foo { x } = foo;
    //     ^ 被推断为 <bool>
}
```

注意：当编译器无法推断类型时，你需要手动标注它们。一个常见的场景是调用一个函数，其类型参数只出现在返回位置。

```move
address 0x2 {
module m {
    using std::vector;

    fun foo() {
        // let v = vector::new();
        //                    ^ 编译器无法确定元素类型。

        let v = vector::new<u64>();
        //                 ^~~~~ 必须手动标注。
    }
}
}
```

但是，如果稍后在该函数中使用该返回值，编译器将能够推断其类型：

```move
address 0x2 {
module m {
    using std::vector;

    fun foo() {
        let v = vector::new();
        //                 ^ 被推断为 <u64>
        vector::push_back(&mut v, 42);
    }
}
}
```

## 未使用的类型参数

对于结构体定义，未使用的类型参数是没有出现在结构体定义的任何字段中，但在编译时静态检查的类型参数。Move 允许未使用的类型参数，因此以下结构体定义有效：

```move
struct Foo<T> {
    foo: u64
}
```

这在对某些概念建模时会很方便。这是一个例子：

```move
address 0x2 {
module m {
    // 货币说明符
    struct Currency1 {}
    struct Currency2 {}

    // 可以使用货币说明符类型实例化的泛型钱币类型。
    // 例如 Coin<Currency1>, Coin<Currency2> 等。
    struct Coin<Currency> has store {
        value: u64
    }

    // 泛型地编写有关所有货币的代码
    public fun mint_generic<Currency>(value: u64): Coin<Currency> {
        Coin { value }
    }

    // 具体编写关于一种货币的代码
    public fun mint_concrete(value: u64): Coin<Currency1> {
        Coin { value }
    }
}
}
```

在此示例中，`struct Coin<Currency>` 是类型参数为 `Currency` 的泛型结构体，该参数指定钱币的货币（类型），并允许将代码泛型地写入任何货币或具体地写入特定货币。即使 `Currency` 类型参数未出现在 `Coin` 中定义的任何字段中，这种通用性也适用。

### 虚类型参数

在上面的例子中，虽然 `struct Coin` 要求有 `store` 能力，但 `Coin<Currency1>` 和 `Coin<Currency2>` 都没有 `store` 能力。这实际是因为[条件能力与泛型类型](./abilities.md#条件能力与泛型类型)的规则以及 `Currency1` 和 `Currency2` 没有 `store` 能力，尽管它们甚至没有在 `struct Coin` 的结构体中使用。这可能会导致一些不合意的后果。例如，我们无法将 `Coin<Currency1>` 放入全局存储中的钱包。

一种可能的解决方案是向 `Currency1` 和 `Currency2` 添加伪能力（spurious ability）标注（例如：`struct Currency1 has store {}`）。但是，这可能会导致错误（bug）或安全漏洞，因为它削弱了类型，引入了不必要的能力声明。例如，我们永远不会期望全局存储中的资源有一个类型为 `Currency1` 的字段，但是通过伪 `store` 能力这是有可能的。此外，伪标注具有传染性，需要在许多未使用类型参数的泛型函数上也包含必要的约束。

虚类型（phantom type）参数解决了这个问题。未使用的类型参数可以标记为 *phantom* 类型参数，不参与结构体的能力推导。这样，在派生泛型类型的能力时，不考虑虚类型参数的实参，从而避免了对伪能力标注的需要。为了使这个宽松的规则合理，Move 的类型系统保证声明为 `phantom` 的参数要么在结构体定义根本不使用，要么仅用作声明为 `phantom` 的类型参数的实参。

#### 声明

在结构定义中，可以通过在声明前添加 `phantom` 关键字来将类型参数声明为 phantom。如果一个类型参数被声明为 phantom，我们就说它是一个虚类型参数。在定义结构时，Move 的类型检查器确保每个虚类型参数要么未在结构定义中使用，要么仅用作虚类型参数的实参。

更正式地说，如果一个类型被用作虚类型参数的实参，我们说该类型出现在_虚位置_。有了这个定义，正确使用虚参数的规则可以指定如下：**虚类型参数只能出现在虚位置**。

以下两个示例显示了虚参数的合法使用。在第一个中，结构定义中根本没有使用参数 `T1`。在第二个中，参数 `T1` 仅用作虚类型参数的实参。

```move
struct S1<phantom T1, T2> { f: u64 }
                  ^^
                  Ok: T1 没有出现在结构定义中

struct S2<phantom T1, T2> { f: S1<T1, T2> }
                                  ^^
                                  Ok: T1 出现在虚位置
```

以下代码展示违反规则的示例：

```move
struct S1<phantom T> { f: T }
                          ^
                          错误：不是虚位置

struct S2<T> { f: T }

struct S3<phantom T> { f: S2<T> }
                             ^
                             错误：不是虚位置
```

#### 实例化

实例化结构时，在派生结构能力时排除虚参数的实参。例如，考虑以下代码：

```move
struct S<T1, phantom T2> has copy { f: T1 }
struct NoCopy {}
struct HasCopy has copy {}
```

现在考虑类型 `S<HasCopy, NoCopy>`。因为 `S` 是用 `copy` 定义的，并且所有非虚参数都有 copy 能力，所以 `S<HasCopy, NoCopy>` 也有 copy 能力。

#### 具有能力约束的虚类型参数

能力约束和虚类型参数是正交特征，虚参数可以用能力约束来声明。当实例化具有能力约束的虚类型参数时，类型实参必须满足该约束，即使该参数是虚的（phantom）。例如，以下定义是完全有效的：

```move
struct S<phantom T: copy> {}
```

通常用来限制应用并且 `T` 只能用具有 `copy` 的实参实例化。

## 约束

在上面的示例中，我们演示了如何使用类型参数来定义稍后可以由调用者插入的“未知”类型。然而，这意味着类型系统几乎没有关于类型的信息，并且必须以非常保守的方式执行检查。在某种意义上，类型系统必须为不受约束的泛型假设最坏的情况。简单地说，默认泛型类型参数没有[能力](./abilities.md)。

这就是约束发挥作用的地方：它们提供了一种方法来指定这些未知类型具有哪些属性，以便类型系统可以允许在其他情况下不安全的操作。

### 声明约束

可以使用以下语法对类型参数施加约束。

```move
// T 是类型参数的名称
T: <ability> (+ <ability>)*
```

`<ability>` 可以是四种[能力](./abilities.md)中的任何一种，一个类型参数可以同时被多种能力约束。因此，以下所有内容都是有效的类型参数声明：

```move
T: copy
T: copy + drop
T: copy + drop + store + key
```

### 验证约束

在调用点检查约束，所以下面的代码不会编译。

```move
struct Foo<T: key> { x: T }

struct Bar { x: Foo<u8> }
//                  ^ 错误！u8 没有 'key'

struct Baz<T> { x: Foo<T> }
//                     ^ 错误！ t 没有 'key'
```

```move
struct R {}

fun unsafe_consume<T>(x: T) {
    // 错误！x 没有 'drop'
}

fun consume<T: drop>(x: T) {
    // 合法！
    // x 会被自动删除
}

fun foo() {
    let r = R {};
    consume<R>(r);
    //      ^ 错误！r 没有 'drop'
}
```

```move
struct R {}

fun unsafe_double<T>(x: T) {
    (copy x, x)
    // 错误！x 没有 'copy'
}

fun double<T: copy>(x: T) {
    (copy x, x) // 合法！
}

fun foo(): (R, R) {
    let r = R {};
    double<R>(r)
    //     ^ 错误！R 没有 'error'
}
```

有关详细信息，请参阅有关[条件能力与泛型类型](./abilities.md#conditional-abilities-and-generic-types)。

## 递归的限制

### 递归结构体

泛型结构不能直接或间接包含相同类型的字段，即使具有不同类型的参数也是如此。以下所有结构定义均无效：

```move
struct Foo<T> {
    x: Foo<u64> // 错误！'Foo' 包含 'Foo'
}

struct Bar<T> {
    x: Bar<T> // 错误！'Bar' 包含 'Bar'
}

// 错误！'A' 和 'B' 形成一个循环，这也是不允许的。
struct A<T> {
    x: B<T, u64>
}

struct B<T1, T2> {
    x: A<T1>
    y: A<T2>
}
```

### 高级主题：类型级递归

Move 允许递归调用泛型函数。然而，当与泛型结构体结合使用时，在某些情况下这可能会创建无限数量的类型，这意味着会给编译器、虚拟机（mv）和其他语言组件增加不必要的复杂性。因此，这样的递归是被禁止的。

被允许的用法：

```move
address 0x2 {
module m {
    struct A<T> {}

    // 有限多种类型 —— 允许。
    // foo<T> -> foo<T> -> foo<T> -> ... is valid
    fun foo<T>() {
        foo<T>();
    }

    // 有限多种类型 —— 允许。
    // foo<T> -> foo<A<u64>> -> foo<A<u64>> -> ... is valid
    fun foo<T>() {
        foo<A<u64>>();
    }
}
}
```

不被允许的用法：

```move
address 0x2 {
module m {
    struct A<T> {}

    // 无限多种类型 —— 不允许。
    // 错误！
    // foo<T> -> foo<A<T>> -> foo<A<A<T>>> -> ...
    fun foo<T>() {
        foo<Foo<T>>();
    }
}
}
```

```move
address 0x2 {
module n {
    struct A<T> {}

    // 无限多种类型 —— 不允许。
    // 错误！
    // foo<T1, T2> -> bar<T2, T1> -> foo<T2, A<T1>>
    //   -> bar<A<T1>, T2> -> foo<A<T1>, A<T2>>
    //   -> bar<A<T2>, A<T1>> -> foo<A<T2>, A<A<T1>>>
    //   -> ...
    fun foo<T1, T2>() {
        bar<T2, T1>();
    }

    fun bar<T1, T2> {
        foo<T1, A<T2>>();
    }
}
}
```

请注意，类型级递归的检查基于对调用点的保守分析，所以不考虑控制流或运行时值。

```move
address 0x2 {
module m {
    struct A<T> {}

    fun foo<T>(n: u64) {
        if (n > 0) {
            foo<A<T>>(n - 1);
        };
    }
}
}
```

上面示例中的函数在技术上将终止任何给定的输入，因此只会创建有限多种类型，但它仍然被 Move 的类型系统视为无效的。
