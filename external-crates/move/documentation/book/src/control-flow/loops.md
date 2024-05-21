# Loop Constructs in Move

Many programs require iteration over values, and Move provides `while` and `loop` forms to allow you
to write code in these situations. In addition, you can also modify control flow of these loops
during execution by using `break` (to exit the loop) and `continue` (to skip the remainder of this
iteration and return to the top of the control flow structure).

## `while` Loops

The `while` construct repeats the body (an expression of type unit) until the condition (an
expression of type `bool`) evaluates to `false`.

Here is an example of simple `while` loop that computes the sum of the numbers from `1` to `n`:

```move
fun sum(n: u64): u64 {
    let mut sum = 0;
    let mut i = 1;
    while (i <= n) {
        sum = sum + i;
        i = i + 1
    };

    sum
}
```

Infinite `while` loops are also allowed:

```move=
fun foo() {
    while (true) { }
}
```

### Using `break` Inside of `while` Loops

In Move, `while` loops can use `break` to exit early. For example, suppose we were looking for the
position of a value in a vector, and would like to `break` if we find it:

```move
fun find_position(values: &vector<u64>, target_value: u64): Option<u64> {
    let size = vector::length(values);
    let mut i = 0;
    let mut found = false;

    while (i < size) {
        if (vector::borrow(values, i) == &target_value) {
            found = true;
            break
        };
        i = i + 1
    };

    if (found) {
        Option::Some(i)
    } else {
        Option::None
    }
}
```

Here, if the borrowed vector value is equal to our target value, we set the `found` flag to `true`
and then call `break`, which will cause the program to exit the loop.

Finally, note that `break` for `while` loops cannot take a value: `while` loops always return the
unit type `()` and thus `break` does, too.

### Using `continue` Inside of `while` Loops

Similar to `break`, Move's `while` loops can invoke `continue` to skip over part of the loop body.
This allows us to skip part of a computation if a condition is not met, such as in the following
example:

```move
fun sum_even(values: &vector<u64>): u64 {
    let size = vector::length(values);
    let mut i = 0;
    let mut even_sum = 0;

    while (i < size) {
        let number = *vector::borrow(values, i);
        i = i + 1;
        if (number % 2 == 1) continue;
        even_sum = even_sum + number;
    };
    even_sum
}
```

This code will iterate over the provided vector. For each entry, if that entry is an even number, it
will add it to the `even_sum`. If it is not, however, it will call `continue`, skipping the sum
operation and returning to the `while` loop conditional check.

## `loop` Expressions

The `loop` expression repeats the loop body (an expression with type `()`) until it hits a `break`:

```move
fun sum(n: u64): u64 {
    let mut sum = 0;
    let mut i = 1;

    loop {
       i = i + 1;
       if (i >= n) break;
       sum = sum + i;
    };

    sum
}
```

Without a `break`, the loop will continue forever. In the example below, the program will run
forever because the `loop` does not have a `break`:

```move
fun foo() {
    let mut i = 0;
    loop { i = i + 1 }
}
```

Here is an example that uses `loop` to write the `sum` function:

```move
fun sum(n: u64): u64 {
    let sum = 0;
    let i = 0;
    loop {
        i = i + 1;
        if (i > n) break;
        sum = sum + i
    };

    sum
}
```

### Using `break` with Values in `loop`

Unlike `while` loops, which always return `()`, a `loop` may return a value using `break`. In doing
so, the overall `loop` expression evaluates to a value of that type. For example, we can rewrite
`find_position` from above using `loop` and `break`, immediately returning the index if we find it:

```move
fun find_position(values: &vector<u64>, target_value: u64): Option<u64> {
    let size = vector::length(values);
    let mut i = 0;

    loop {
        if (vector::borrow(values, i) == &target_value) {
            break Option::Some(i)
        } else if (i >= size) {
            break Option::None
        };
        i = i + 1;
    }
}
```

This loop will break with an option result, and, as the last expression in the function body, will
produce that value as the final function result.

### Using `continue` Inside of `loop` Expressions

As you might expect, `continue` can also be used inside a `loop`. Here is the previous `sum_even`
function rewritten using `loop` with `break `and` continue` instead of `while`.

```move
fun sum_even(values: &vector<u64>): u64 {
    let size = vector::length(values);
    let mut i = 0;
    let mut even_sum = 0;

    loop {
        if (i >= size) break;
        let number = *vector::borrow(values, i);
        i = i + 1;
        if (number % 2 == 1) continue;
        even_sum = even_sum + number;
    };
    even_sum
}
```

## The Type of `while` and `loop`

In Move, loops are typed expressions. A `while` expression always has type `()`.

```move
let () = while (i < 10) { i = i + 1 };
```

If a `loop` contains a `break`, the expression has the type of the break. A break with no value has
the unit type `()`.

```move
(loop { if (i < 10) i = i + 1 else break }: ());
let () = loop { if (i < 10) i = i + 1 else break };

let x: u64 = loop { if (i < 10) i = i + 1 else break 5 };
let x: u64 = loop { if (i < 10) { i = i + 1; continue} else break 5 };
```

In addition, if a loop contains multiple breaks, they must all return the same type:

```move
// invalid -- first break returns (), second returns 5
let x: u64 = loop { if (i < 10) break else break 5 };
```

If `loop` does not have a `break`, `loop` can have any type much like `return`, `abort`, `break`,
and `continue`.

```move
(loop (): u64);
(loop (): address);
(loop (): &vector<vector<u8>>);
```

If you need even more-precise control flow, such as breaking out of nested loops, the next chapter
presents the use of labeled control flow in Move.
