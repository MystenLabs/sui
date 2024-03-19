module Symbols::M2 {

    /// Constant containing the answer to the universe
    const DOCUMENTED_CONSTANT: u64 = 42;

    /**
       This is a multiline docstring

       This docstring has empty lines.

       It uses the ** format instead of ///
    */
    fun other_doc_struct(): Symbols::M3::OtherDocStruct {
        Symbols::M3::create_other_struct(DOCUMENTED_CONSTANT)
    }

    use Symbols::M3::{Self, OtherDocStruct};

    fun other_doc_struct_import(): OtherDocStruct {
        M3::create_other_struct(7)
    }
}
