module ParseError::M1 {
    // these parsing error should not prevent M2 from building symbolication information
    parse_error

    const y =

    const x; u64 = 42;

    const c: u64 = 7;

    const d
}

module ParseError::M3 {
    const c: u64 = 7;

    const d
}

#[test]
module ParseError::M4 {
    const c: u64 = 7;
}
