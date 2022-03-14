module Test::M1 {
    use Std::Vector;

    use Sui::TxContext::TxContext;

    const ADDR: address = @0x42;

    public fun get_u64(_ctx: &mut TxContext): u64 {
        42
    }

    public fun get_addr(_ctx: &mut TxContext): address {
        ADDR
    }

    public fun get_tuple(_ctx: &mut TxContext): (u64, address) {
        (42, ADDR)
    }

    public fun get_vec(_ctx: &mut TxContext): vector<u64> {
        let vec = Vector::empty();
        Vector::push_back(&mut vec, 42);
        Vector::push_back(&mut vec, 7);
        vec
    }

    public fun get_vec_vec(_ctx: &mut TxContext): vector<vector<u64>> {
        let vec = Vector::empty();
        Vector::push_back(&mut vec, 42);
        Vector::push_back(&mut vec, 7);
        let vec2 = Vector::empty();
        Vector::push_back(&mut vec2, vec);
        vec2
    }

}
