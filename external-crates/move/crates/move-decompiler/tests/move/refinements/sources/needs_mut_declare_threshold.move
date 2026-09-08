// An uninitialized binder needs two assignments on some path before it takes `mut`, its
// first being the initializer. The two functions differ only in the extra assignment.
// The second binder in each keeps the pair from collapsing into `let x = if ...`.

module refinements::needs_mut_declare_threshold;

#[allow(unused)]
public fun single_assign_per_path(c: bool): u64 {
    let x;
    let y;
    if (c) { x = 1; y = 2 } else { x = 3; y = 4 };
    x + y
}

#[allow(unused)]
public fun double_assign(c: bool): u64 {
    let mut x;
    let y;
    if (c) { x = 1; y = 2 } else { x = 3; y = 4 };
    if (c) { x = 5 };
    x + y
}
