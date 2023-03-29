#[evm_contract]
module 0x2::M {

    #[callable(sig=b"1add()")]
    fun illegal_char_begin() {
    }

    #[callable(sig=b"add) () ")]
    fun illegal_char() {

    }

}
