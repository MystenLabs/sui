// weird but valid cases for TxContext parameters. The PTB runtime allows

module a::m {
    use sui::tx_context::TxContext;
    entry fun mut_first(_ctx: &mut TxContext, _x: u64) {
    }

    entry fun mut_middle(_x: u64, _ctx: &mut TxContext, _y: u64) {
    }

    entry fun imm_first(_ctx: &TxContext, _x: u64) {
    }

    entry fun imm_middle(_x: u64, _ctx: &TxContext, _y: u64) {
    }

    entry fun two_imm(_ctx1: &TxContext, _ctx2: &TxContext) {
    }

    entry fun two_imm_mixed(_ctx1: &TxContext, _x: u64, _ctx2: &TxContext) {
    }

}


module sui::tx_context {
    struct TxContext has drop {}
}
