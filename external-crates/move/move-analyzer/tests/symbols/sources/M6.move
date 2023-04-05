module Symbols::M6 {

    /// This is a documented struct
    /// With a multi-line docstring
    struct DocumentedStruct has key, drop, store {
        /// A documented field
        documented_field: u64,
    }

    /// Constant containing the answer to the universe
    const DOCUMENTED_CONSTANT: u64 = 42;


    /// A documented function that unpacks a DocumentedStruct
    fun unpack(s: DocumentedStruct): u64 {
        let DocumentedStruct { documented_field: value } = s;
        value
    }

    /**
       This is a multiline docstring

       This docstring has empty lines.

       It uses the ** format instead of ///
    */
    fun other_doc_struct(): Symbols::M7::OtherDocStruct {
        Symbols::M7::create_other_struct(DOCUMENTED_CONSTANT)
    }

    /** Asterix based single-line docstring */
    fun acq(addr: address): u64 acquires DocumentedStruct {
        let val = borrow_global<DocumentedStruct>(addr);
        val.documented_field
    }

    use Symbols::M7::{Self, OtherDocStruct};

    fun other_doc_struct_import(): OtherDocStruct {
        M7::create_other_struct(7)
    }
}
