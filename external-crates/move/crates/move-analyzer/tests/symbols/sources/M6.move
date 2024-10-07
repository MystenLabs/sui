module Symbols::M6 {

    /// This is a documented struct
    /// With a multi-line docstring
    public struct DocumentedStruct has drop, store {
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
    fun acq(uint: u64): u64 {
        uint +
        1
    }

    use Symbols::M7::{Self, OtherDocStruct};

    fun other_doc_struct_import(): OtherDocStruct {
        M7::create_other_struct(7)
    }

    /// A documented function with type params.
    fun type_param_doc<T: copy + drop>(param: T): T {
        param
    }

    /// A documented function with code block
    /// (should preserve indentation in the code block)
    ///
    /// ```rust
    /// fun foo() {
    ///     42
    /// }
    /// ```
    fun code_block_doc_slash() {}

    /**
       A documented function with code block
       (should preserve indentation in the code block)

       ```rust
       fun foo() {
           42
       }
       ```
    */
    fun code_block_doc_star() {}

    /**
      Misformatted docstring to have fewer whitespace in the body than
      at the ending marker.


  Beginning of this string should not disappear.

Beginning of this string should not disappear either.

    */
    fun misformatted_docstring() {}


    /// Docstring before attributes
    #[test_only]
    fun attributes_after_docstring() {}

    #[test_only]
    /// Docstring after attributes
    fun attributes_before_docstring() {}

}
