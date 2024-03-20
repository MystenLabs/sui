# 常量 (Constants)

Constants are a way of giving a name to shared, static values inside of a `module` or `script`.

The constant's must be known at compilation. The constant's value is stored in the compiled module
or script. And each time the constant is used, a new copy of that value is made.

常量是一种对 `module` 或 `script` 内的共享静态值进行命名的方法(类似变量，但值不变，译者注)。

常量必须在编译时知道。常量的值存储在编译模块或脚本中。每次使用该常量时，都会生成该值的新副本。

## 声明 (Declaration)

Constant declarations begin with the `const` keyword, followed by a name, a type, and a value. They
can exist in either a script or module

常量声明以 `const` 关键字开头，后跟名称、类型和值。他们可以存在于脚本或模块中

```text
const <name>: <type> = <expression>;
```

例如

```move=
script {

    const MY_ERROR_CODE: u64 = 0;

    fun main(input: u64) {
        assert!(input > 0, MY_ERROR_CODE);
    }

}

address 0x42 {
    module example {

        const MY_ADDRESS: address = @0x42;

        public fun permissioned(s: &signer) {
            assert!(std::signer::address_of(s) == MY_ADDRESS, 0);
        }

    }
}
```

## 命名 (Naming)

Constants must start with a capital letter `A` to `Z`. After the first letter, constant names can
contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

常量必须以大写字母 `A` 到 `Z` 开头。在第一个字母之后，常量名可以包含下划线 `_`、字母 `a` 到 `z`、字母 `A` 到 `Z` 或数字 `0` 到 `9`。

```move
const FLAG: bool = false;
const MY_ERROR_CODE: u64 = 0;
const ADDRESS_42: address = @0x42;
```

Even though you can use letters `a` to `z` in a constant. The
[general style guidelines](./coding-conventions.md) are to use just uppercase letters `A` to `Z`,
with underscores `_` between each word.

虽然你可以在常量中使用字母 `a` 到 `z`。但[通用风格指南](./coding-conventions.md) 只使用大写字母 `A` 到 `Z`，每个单词之间有下划线`_`。

This naming restriction of starting with `A` to `Z` is in place to give room for future language features. It may or may not be removed later.

这种以 `A` 到 `Z` 开头的命名限制是为了给未来的语言特性留出空间。此限制未来可能会保留或删除。

## 可见性 (Visibility)

`public` constants are not currently supported. `const` values can be used only in the declaring
module.

当前不支持 `public` 常量。 `const` 值只能在声明的模块中使用。

## 有效表达式 (Valid Expressions)

Currently, constants are limited to the primitive types `bool`, `u8`, `u64`, `u128`, `address`, and
`vector<u8>`. Future support for other `vector` values (besides the "string"-style literals) will come later.

目前，常量仅限于原始类型 `bool`、`u8`、`u64`、`u128`、`address` 和`vector<u8>`。其他 `vector` 值(除了“string”风格的字面量)将在不远的将来获得支持。

### 值 (Values)

Commonly, `const`s are assigned a simple value, or literal, of their type. For example

通常，`const` (常量)会被分配一个对应类型的简单值或字面量。例如

```move
const MY_BOOL: bool = false;
const MY_ADDRESS: address = @0x70DD;
const BYTES: vector<u8> = b"hello world";
const HEX_BYTES: vector<u8> = x"DEADBEEF";
```

### 复杂表达式 (Complex Expressions)


In addition to literals, constants can include more complex expressions, as long as the compiler is
able to reduce the expression to a value at compile time.

Currently, equality operations, all boolean operations, all bitwise operations, and all arithmetic
operations can be used.

除了字面量，常量还可以包含更复杂的表达式，只要编译器能够在编译时将表达式归纳(reduce)为一个值。

目前，相等运算、所有布尔运算、所有按位运算和所有算术运算可以使用。

```move
const RULE: bool = true && false;
const CAP: u64 = 10 * 100 + 1;
const SHIFTY: u8 = {
  (1 << 1) * (1 << 2) * (1 << 3) * (1 << 4)
};
const HALF_MAX: u128 = 340282366920938463463374607431768211455 / 2;
const EQUAL: bool = 1 == 1;
```

If the operation would result in a runtime exception, the compiler will give an error that it is
unable to generate the constant's value

如果操作会导致运行时异常，编译器会给出无法生成常量值的错误。

```move
const DIV_BY_ZERO: u64 = 1 / 0; // error!
const SHIFT_BY_A_LOT: u64 = 1 << 100; // error!
const NEGATIVE_U64: u64 = 0 - 1; // error!
```

Note that constants cannot currently refer to other constants. This feature, along with support for
other expressions, will be added in the future.

请注意，常量当前不能引用其他常量。此功能会在将来和支持其他表达方式一起被补充。
