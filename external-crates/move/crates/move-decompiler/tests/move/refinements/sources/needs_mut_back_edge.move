// The back edge repeats the assignment, so one assignment in the body earns `mut`.

module refinements::needs_mut_back_edge;

#[allow(unused)]
public fun assign_reaching_back_edge(n: u64): u64 {
    let mut i = 0;
    while (i < n) { i = i + 1 };
    i
}
