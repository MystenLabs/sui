// valid, ASCII and UTF strings is allowed

module a::m {
    use sui::tx_context;
    use std::ascii;
    use std::string;

    public entry fun yes_ascii<T>(
        _: ascii::String,
        _: vector<ascii::String>,
        _: vector<vector<ascii::String>>,
        _: &mut tx_context::TxContext,
    ) {
        abort 0
    }

    public entry fun yes_utf8<T>(
        _: string::String,
        _: vector<string::String>,
        _: vector<vector<string::String>>,
        _: &mut tx_context::TxContext,
    ) {
        abort 0
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
