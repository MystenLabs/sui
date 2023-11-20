module a::m {
    public struct X {}
    public struct Y {}

    public fun val_u64(_: u64) { abort 0 }
    public fun imm_vec(_: &vector<u64>) { abort 0 }
    public fun mut_addr(_: &mut address) { abort 0 }

    public fun val_x(_: X) { abort 0 }
    public fun imm_x(_: &X) { abort 0 }
    public fun mut_x(_: &mut X) { abort 0 }

    public fun val_gen<T>(_: T) { abort 0 }
    public fun imm_gen<T>(_: &T) { abort 0 }
    public fun mut_gen<T>(_: &mut T) { abort 0 }


    use fun val_u64 as Y.val_u64;
    use fun imm_vec as Y.imm_vec;
    use fun mut_addr as Y.mut_addr;

    use fun val_x as Y.val_x;
    use fun imm_x as Y.imm_x;
    use fun mut_x as Y.mut_x;

    use fun val_gen as Y.val_gen;
    use fun imm_gen as Y.imm_gen;
    use fun mut_gen as Y.mut_gen;
}
