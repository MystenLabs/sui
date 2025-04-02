#[allow(ide_path_autocomplete)]
module a::m {

    public struct SomeStruct {
        some_field: u64,
    }

    public fun bar(_some_struct: &SomeStruct):u64 {
        42
    }

    public fun foo(some_struct: &mut SomeStruct): u64 {
        // Auto-completion after some_struct. did not work here before,
        // but it worked if the other some_struct access
        // is not a dot call but rather a field access
        some_struct.
        some_struct.bar()
    }

}
