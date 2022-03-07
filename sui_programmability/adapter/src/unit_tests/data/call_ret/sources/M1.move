module Test::M1 {
    use Sui::TxContext::TxContext;

    public fun identity_u64(value: u64, _ctx: &mut TxContext): u64 {
        value
    }    
}
