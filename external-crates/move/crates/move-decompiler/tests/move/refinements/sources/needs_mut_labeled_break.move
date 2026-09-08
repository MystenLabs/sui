// The same as `needs_mut_assign_then_break`, from an inner loop leaving both.

module refinements::needs_mut_labeled_break;

#[allow(unused)]
public fun labeled_break_exits_both_loops(n: u64, m: u64): u64 {
    let x;
    let mut i = 0;
    'outer: loop {
        let mut j = 0;
        loop {
            if (i + j >= n) { x = i + j; break 'outer };
            if (j >= m) break;
            j = j + 1
        };
        i = i + 1
    };
    if (x > m) { x - m } else { m - x }
}
