address 0x2 {

module N {
    fun foo() {}
}

module M {
    use 0x2::N::{
        foo as address,
        foo as signer,
        foo as u8,
        foo as u64,
        foo as u128,
        foo as vector,
        foo as freeze,
        foo as assert,
        foo as Self,
    };
}

}
