// return values from entry functions must have drop

module a::m {
    public entry fun t0(): &u8 {
        abort 0
    }
    public entry fun t1(): &mut u8 {
        abort 0
    }
    public entry fun t2(): (u64,&u8,u8) {
        abort 0
    }
    struct Copyable has copy, store {}
    public entry fun t3(): Copyable {
        abort 0
    }
    struct Obj has key, store { id: sui::object::UID }
    public entry fun t4(): Obj {
        abort 0
    }
    public entry fun t5(): vector<Obj> {
        abort 0
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
