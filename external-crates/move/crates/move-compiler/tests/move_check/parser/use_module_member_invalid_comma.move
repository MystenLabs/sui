
module 0x42::M {

    use 0x1::X::{S as XS,,};

    fun foo(_s: &XS) {}

}

module  0x1::X {
    struct S {}
}
