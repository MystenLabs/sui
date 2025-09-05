#[allow(unused)]
/// This is a doc comment on a module
module summary_pkg::a;

/// This is a doc comment on a struct
public struct X {
    /// This is a doc comment on a field
    x: u64,
    y: 0xc0ffee::b::X,

}

public fun f<Typename1, Typename2>(_param_1: Typename1, _param_2: Typename2): Typename1 {
    abort
}
