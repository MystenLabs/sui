// Constant pattern referenced through a fully-qualified path. The existing
// `let_else_constant` test uses a bare `MY_CONST`; this exercises the
// multi-segment `name_access_chain_to_module_access` path.
module 0x42::m {
    const MY_CONST: u64 = 42;

    fun qualified_const(x: u64): u64 {
        let 0x42::m::MY_CONST = x else { return 0 };
        1
    }
}
