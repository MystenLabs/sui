// filter out dot calls to functions from a spec_only module

module a::m {
    #[verify_only]
    use std::integer::{Integer, Real};

    public fun foo() {
        let x = x.to_int();
        let x = x.to_real();
    }
}
