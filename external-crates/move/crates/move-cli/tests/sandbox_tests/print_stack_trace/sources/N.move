#[allow(unused_type_parameter, unused_mut_ref)]
module 0x2::N {
    use 0x2::M;

    public fun foo<T1, T2>(): u64 {
        let mut x = 3;
        let y = &mut x;
        let z = M::sum(4);
        _ = y;
        z
    }
}
