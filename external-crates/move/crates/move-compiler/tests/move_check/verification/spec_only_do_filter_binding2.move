// filter out let bindings calling functions from a spec_only module

module a::m {
    #[verify_only]
    use std::integer::Integer;

    public fun foo(_x: u64) {
        let x0 = _x.toInt();
    }
}
