# Generics

Generics can be used to define functions and structs over different input data types. This language
feature is sometimes referred to as parametric polymorphism. In Move, we will often use the term
generics interchangeably with _type parameters_ and _type arguments_.

Generics are commonly used in library code, such as in [vector](./primitive-types/vector.md), to
declare code that works over any possible type (that satisfies the specified constraints). This sort
of parameterization allows you to reuse the same implementation across multiple types and
situations.

## Declaring Type Parameters

Both functions and structs can take a list of type parameters in their signatures, enclosed by a
pair of angle brackets `<...>`.

### Generic Functions

Type parameters for functions are placed after the function name and before the (value) parameter
list. The following code defines a generic identity function that takes a value of any type and
returns that value unchanged.

```move
fun id<T>(x: T): T {
    // this type annotation is unnecessary but valid
    (x: T)
}
```

Once defined, the type parameter `T` can be used in parameter types, return types, and inside the
function body.

### Generic Structs

Type parameters for structs are placed after the struct name, and can be used to name the types of
the fields.

```move
public struct Foo<T> has copy, drop { x: T }

public struct Bar<T1, T2> has copy, drop {
    x: T1,
    y: vector<T2>,
}
```

Note that [type parameters do not have to be used](#unused-type-parameters)

## Type Arguments

### Calling Generic Functions

When calling a generic function, one can specify the type arguments for the function's type
parameters in a list enclosed by a pair of angle brackets.

```move
fun foo() {
    let x = id<bool>(true);
}
```

If you do not specify the type arguments, Move's [type inference](#type-inference) will supply them
for you.

### Using Generic Structs

Similarly, one can attach a list of type arguments for the struct's type parameters when
constructing or destructing values of generic types.

```move
fun foo() {
    // type arguments on construction
    let foo = Foo<bool> { x: true };
    let bar = Bar<u64, u8> { x: 0, y: vector<u8>[] };

    // type arguments on destruction
    let Foo<bool> { x } = foo;
    let Bar<u64, u8> { x, y } = bar;
}
```

In any case if you do not specify the type arguments, Move's [type inference](#type-inference) will
supply them for you.

### Type Argument Mismatch

If you specify the type arguments and they conflict with the actual values supplied, an error will
be given:

```move
fun foo() {
    let x = id<u64>(true); // ERROR! true is not a u64
}
```

and similarly:

```move
fun foo() {
    let foo = Foo<bool> { x: 0 }; // ERROR! 0 is not a bool
    let Foo<address> { x } = foo; // ERROR! bool is incompatible with address
}
```

## Type Inference

In most cases, the Move compiler will be able to infer the type arguments so you don't have to write
them down explicitly. Here's what the examples above would look like if we omit the type arguments:

```move
fun foo() {
    let x = id(true);
    //        ^ <bool> is inferred

    let foo = Foo { x: true };
    //           ^ <bool> is inferred

    let Foo { x } = foo;
    //     ^ <bool> is inferred
}
```

Note: when the compiler is unable to infer the types, you'll need annotate them manually. A common
scenario is to call a function with type parameters appearing only at return positions.

```move
module a::m {

    fun foo() {
        let v = vector[]; // ERROR!
        //            ^ The compiler cannot figure out the element type, since it is never used

        let v = vector<u64>[];
        //            ^~~~~ Must annotate manually in this case.
    }
}
```

Note that these cases are a bit contrived since the `vector[]` is never used, ad as such, Move's
type inference cannot infer the type.

However, the compiler will be able to infer the type if that value is used later in that function:

```move
module a::m {
    fun foo() {
        let v = vector[];
        //            ^ <u64> is inferred
        vector::push_back(&mut v, 42);
        //               ^ <u64> is inferred
    }
}
```

## Integers

In Move, the integer types `u8`, `u16`, `u32`, `u64`, `u128`, and `u256` are all distinct types.
However, each one of these types can be created with the same numerical value syntax. In other
words, if a type suffix is not provided, the compiler will infer the integer type based on the usage
of the value.

```move
let x8: u8 = 0;
let x16: u16 = 0;
let x32: u32 = 0;
let x64: u64 = 0;
let x128: u128 = 0;
let x256: u256 = 0;
```

If the value is not used in a context that requires a specific integer type, `u64` is taken as a
default.

```move
let x = 0;
//      ^ u64 is used by default
```

If the value however is too large for the inferred type, an error will be given

```move
let i: u8 = 256; // ERROR!
//          ^^^ too large for u8
let x = 340282366920938463463374607431768211454;
//      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ too large for u64
```

In cases where the number is too large, you might need to annotate it explicitly

```move
let x = 340282366920938463463374607431768211454u128;
//                                             ^^^^ valid!
```

## Unused Type Parameters

For a struct definition, an unused type parameter is one that does not appear in any field defined
in the struct, but is checked statically at compile time. Move allows unused type parameters so the
following struct definition is valid:

```move
public struct Foo<T> {
    foo: u64
}
```

This can be convenient when modeling certain concepts. Here is an example:

```move
module a::m {
    // Currency Specifiers
    public struct A {}
    public struct B {}

    // A generic coin type that can be instantiated using a currency
    // specifier type.
    //   e.g. Coin<A>, Coin<B> etc.
    public struct Coin<Currency> has store {
        value: u64
    }

    // Write code generically about all currencies
    public fun mint_generic<Currency>(value: u64): Coin<Currency> {
        Coin { value }
    }

    // Write code concretely about one currency
    public fun mint_a(value: u64): Coin<A> {
        mint_generic(value)
    }
    public fun mint_b(value: u64): Coin<B> {
        mint_generic(value)
    }
}
```

In this example, `Coin<Currency>` is generic on the `Currency` type parameter, which specifies the
currency of the coin and allows code to be written either generically on any currency or concretely
on a specific currency. This generality applies even when the `Currency` type parameter does not
appear in any of the fields defined in `Coin`.

### Phantom Type Parameters

In the example above, although `struct Coin` asks for the `store` ability, neither `Coin<A>` nor
`Coin<B>` will have the `store` ability. This is because of the rules for
[Conditional Abilities and Generic Types](./abilities.md#conditional-abilities-and-generic-types)
and the fact that `A` and `B` don't have the `store` ability, despite the fact that they are not
even used in the body of `struct Coin`. This might cause some unpleasant consequences. For example,
we are unable to put `Coin<A>` into a wallet in storage.

One possible solution would be to add spurious ability annotations to `A` and `B` (i.e.,
`public struct Currency1 has store {}`). But, this might lead to bugs or security vulnerabilities
because it weakens the types with unnecessary ability declarations. For example, we would never
expect a value in the storage to have a field in type `A`, but this would be possible with the
spurious `store` ability. Moreover, the spurious annotations would be infectious, requiring many
functions generic on the unused type parameter to also include the necessary constraints.

Phantom type parameters solve this problem. Unused type parameters can be marked as _phantom_ type
parameters, which do not participate in the ability derivation for structs. In this way, arguments
to phantom type parameters are not considered when deriving the abilities for generic types, thus
avoiding the need for spurious ability annotations. For this relaxed rule to be sound, Move's type
system guarantees that a parameter declared as `phantom` is either not used at all in the struct
definition, or it is only used as an argument to type parameters also declared as `phantom`.

#### Declaration

In a struct definition a type parameter can be declared as phantom by adding the `phantom` keyword
before its declaration.

```move
public struct Coin<phantom Currency> has store {
    value: u64
}
```

If a type parameter is declared as phantom we say it is a phantom type parameter. When defining a
struct, Move's type checker ensures that every phantom type parameter is either not used inside the
struct definition or it is only used as an argument to a phantom type parameter.

```move
public struct S1<phantom T1, T2> { f: u64 }
//               ^^^^^^^ valid, T1 does not appear inside the struct definition

public struct S2<phantom T1, T2> { f: S1<T1, T2> }
//               ^^^^^^^ valid, T1 appears in phantom position
```

The following code shows examples of violations of the rule:

```move
public struct S1<phantom T> { f: T }
//               ^^^^^^^ ERROR!  ^ Not a phantom position

public struct S2<T> { f: T }
public struct S3<phantom T> { f: S2<T> }
//               ^^^^^^^ ERROR!     ^ Not a phantom position
```

More formally, if a type is used as an argument to a phantom type parameter we say the type appears
in _phantom position_. With this definition in place, the rule for the correct use of phantom
parameters can be specified as follows: **A phantom type parameter can only appear in phantom
position**.

Note that specifying `phantom` is not required, but the compiler will warn if a type parameter could
be `phantom` but was not marked as such.

#### Instantiation

When instantiating a struct, the arguments to phantom parameters are excluded when deriving the
struct abilities. For example, consider the following code:

```move
public struct S<T1, phantom T2> has copy { f: T1 }
public struct NoCopy {}
public struct HasCopy has copy {}
```

Consider now the type `S<HasCopy, NoCopy>`. Since `S` is defined with `copy` and all non-phantom
arguments have `copy` then `S<HasCopy, NoCopy>` also has `copy`.

#### Phantom Type Parameters with Ability Constraints

Ability constraints and phantom type parameters are orthogonal features in the sense that phantom
parameters can be declared with ability constraints.

```move
public struct S<phantom T: copy> {}
```

When instantiating a phantom type parameter with an ability constraint, the type argument has to
satisfy that constraint, even though the parameter is phantom. The usual restrictions apply and `T`
can only be instantiated with arguments having `copy`.

## Constraints

In the examples above, we have demonstrated how one can use type parameters to define "unknown"
types that can be plugged in by callers at a later time. This however means the type system has
little information about the type and has to perform checks in a very conservative way. In some
sense, the type system must assume the worst case scenario for an unconstrained generic--a type with
no [abilities](./abilities.md).

Constraints offer a way to specify what properties these unknown types have so the type system can
allow operations that would otherwise be unsafe.

### Declaring Constraints

Constraints can be imposed on type parameters using the following syntax.

```move
// T is the name of the type parameter
T: <ability> (+ <ability>)*
```

The `<ability>` can be any of the four [abilities](./abilities.md), and a type parameter can be
constrained with multiple abilities at once. So all of the following would be valid type parameter
declarations:

```move
T: copy
T: copy + drop
T: copy + drop + store + key
```

### Verifying Constraints

Constraints are checked at instantiation sites

```move
public struct Foo<T: copy> { x: T }

public struct Bar { x: Foo<u8> }
//                         ^^ valid, u8 has `copy`

public struct Baz<T> { x: Foo<T> }
//                            ^ ERROR! T does not have 'copy'
```

And similarly for functions

```move
fun unsafe_consume<T>(x: T) {
    // ERROR! x does not have 'drop'
}

fun consume<T: drop>(x: T) {
    // valid, x will be dropped automatically
}

public struct NoAbilities {}

fun foo() {
    let r = NoAbilities {};
    consume<NoAbilities>(NoAbilities);
    //      ^^^^^^^^^^^ ERROR! NoAbilities does not have 'drop'
}
```

And some similar examples, but with `copy`

```move
fun unsafe_double<T>(x: T) {
    (copy x, x)
    // ERROR! T does not have 'copy'
}

fun double<T: copy>(x: T) {
    (copy x, x) // valid, T has 'copy'
}

public struct NoAbilities {}

fun foo(): (NoAbilities, NoAbilities) {
    let r = NoAbilities {};
    double<NoAbilities>(r)
    //     ^ ERROR! NoAbilities does not have 'copy'
}
```

For more information, see the abilities section on
[conditional abilities and generic types](./abilities.md#conditional-abilities-and-generic-types).

## Limitations on Recursions

### Recursive Structs

Generic structs can not contain fields of the same type, either directly or indirectly, even with
different type arguments. All of the following struct definitions are invalid:

```move
public struct Foo<T> {
    x: Foo<u64> // ERROR! 'Foo' containing 'Foo'
}

public struct Bar<T> {
    x: Bar<T> // ERROR! 'Bar' containing 'Bar'
}

// ERROR! 'A' and 'B' forming a cycle, which is not allowed either.
public struct A<T> {
    x: B<T, u64>
}

public struct B<T1, T2> {
    x: A<T1>
    y: A<T2>
}
```

### Advanced Topic: Type-level Recursions

Move allows generic functions to be called recursively. However, when used in combination with
generic structs, this could create an infinite number of types in certain cases, and allowing this
means adding unnecessary complexity to the compiler, vm and other language components. Therefore,
such recursions are forbidden.

This restriction might be relaxed in the future, but for now, the following examples should give you
an idea of what is allowed and what is not.

```move
module a::m {
    public struct A<T> {}

    // Finitely many types -- allowed.
    // foo<T> -> foo<T> -> foo<T> -> ... is valid
    fun foo<T>() {
        foo<T>();
    }

    // Finitely many types -- allowed.
    // foo<T> -> foo<A<u64>> -> foo<A<u64>> -> ... is valid
    fun foo<T>() {
        foo<A<u64>>();
    }
}
```

Not allowed:

```move
module a::m {
    public struct A<T> {}

    // Infinitely many types -- NOT allowed.
    // error!
    // foo<T> -> foo<A<T>> -> foo<A<A<T>>> -> ...
    fun foo<T>() {
        foo<Foo<T>>();
    }
}
```

And similarly, not allowed:

```move
module a::n {
    public struct A<T> {}

    // Infinitely many types -- NOT allowed.
    // error!
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
```

Note, the check for type level recursions is based on a conservative analysis on the call sites and
does NOT take control flow or runtime values into account.

```move
module a::m {
    public struct A<T> {}

    // Infinitely many types -- NOT allowed.
    // error!
    fun foo<T>(n: u64) {
        if (n > 0) foo<A<T>>(n - 1);
    }
}
```

The function in the example above will technically terminate for any given input and therefore only
creating finitely many types, but it is still considered invalid by Move's type system.
