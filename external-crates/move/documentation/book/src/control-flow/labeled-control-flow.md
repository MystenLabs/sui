# Labeled Control Flow

Move supports labeled control flow, allowing you to define and transfer control to specific labels
in a function. For example, we can nest two loops and use `break` and `continue` with those labels
to precisely specify control flow. You can prefix any `loop` or `while` form with a `'label:` form
to allow breaking or continuing directly there.

To demonstrate this behavior, consider a function that takes nested vectors of numbers (i.e.,
`vector<vector<u64>>`) to sum against some threshold, which behaves as follows:

- If the sum of all the numbers are under the threshold, return that sum.
- If adding a number to the current sum would surpass the threshold, return the current sum.

We can write this by iterating over the vector of vectors as nested loops and labelling the outer
one. If any addition in the inner loop would push us over the threshold, we can use `break` with the
outer label to escape both loops at once:

```move
fun sum_until_threshold(input: &vector<vector<u64>>, threshold: u64): u64 {
    let mut sum = 0;
    let mut i = 0;
    let input_size = vector::length(vec);

    'outer: loop {
        // breaks to outer since it is the closest enclosing loop
        if (i >= input_size) break sum;

        let vec = vector::borrow(input, i);
        let size = vector::length(vec);
        let mut j = 0;

        while (j < size) {
            let v_entry = *vector::borrow(vec, j);
            if (sum + v_entry < threshold) {
                sum = sum + v_entry;
            } else {
                // the next element we saw would break the threshold,
                // so we return the current sum
                break 'outer sum
            }
            j = j + 1;
        }
        i = i + 1;
    }
}
```

These sorts of labels can also be used with a nested loop form, providing precise control in larger
bodies of code. For example, if we were processing a large table where each entry required iteration
that might see us continuing the inner or outer loop, we could express that code using labels:

```move
'outer: loop {
    ...
    'inner: while (cond) {
        ...
        if (cond0) { break 'outer value }
        ...
        if (cond1) { continue 'inner }
        else if (cond2) { continue 'outer }
        ...
    }
    ...
}
```
