// A binder that is read but never written takes no `mut`. Two reads keep
// `inline_immutable_alias` from collapsing the binding away before the analysis sees it.

module refinements::needs_mut_unassigned_let;

#[allow(unused)]
public fun unassigned_let(n: u64): u64 {
    let x = double(n);
    x + x
}

fun double(p: u64): u64 { p + p }
