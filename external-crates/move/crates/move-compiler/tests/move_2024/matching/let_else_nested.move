module 0x42::m {

    public enum Outer has drop {
        Some(Inner),
        None,
    }

    public enum Inner has drop {
        Val(u64),
        Empty,
    }

    fun nested(o: Outer): u64 {
        let Outer::Some(Inner::Val(x)) = o else { return 0 };
        x
    }

}
