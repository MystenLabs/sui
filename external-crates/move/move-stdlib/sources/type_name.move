/// Functionality for converting Move types into values. Use with care!
module std::type_name {
    use std::ascii::String;

    struct TypeName has copy, drop, store {
        /// String representation of the type. All types are represented
        /// using their source syntax:
        /// "u8", "u64", "u128", "bool", "address", "vector", "signer" for ground types.
        /// Struct types are represented as fully qualified type names; e.g.
        /// `00000000000000000000000000000001::string::String` or
        /// `0000000000000000000000000000000a::module_name1::type_name1<0000000000000000000000000000000a::module_name2::type_name2<u64>>`
        /// Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
        name: String
    }

    /// Return a value representation of the type `T`.
    public native fun get<T>(): TypeName;

    /// Get the String representation of `self`
    public fun borrow_string(self: &TypeName): &String {
        &self.name
    }

    /// Convert `self` into its inner String
    public fun into_string(self: TypeName): String {
        self.name
    }
}
