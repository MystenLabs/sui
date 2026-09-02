module ConstDeps::m {
    const BASE: u64 = ConstDeps::defs::MAX + 1;

    public fun base(): u64 { BASE }
}
