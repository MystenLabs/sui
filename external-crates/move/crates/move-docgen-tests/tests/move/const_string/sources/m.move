#[allow(unused)]
module a::m {
    #[error]
    /// This is a doc comment above an error constant that should be rendered as a string
    const AString: vector<u8> = b"Hello, world  🦀   ";

    #[error]
    /// This is a doc comment above an error constant that should not be rendered as a string
    const ErrorNotString: u64 = 10;

    const AStringNotError: vector<u8> = b"Hello, world  🦀   ";

    const NotAString: vector<u8> = vector[1, 2, 3];
}
