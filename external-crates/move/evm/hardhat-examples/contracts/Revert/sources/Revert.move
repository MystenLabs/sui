#[evm_contract]
module 0x1::Revert {
    use Evm::U256::{u256_from_u128, U256};
    use Evm::Evm::abort_with;

    #[callable, pure]
    public fun revertIf0(x: u64) {
        if (x == 0) {
            abort(0);
        }
    }

    #[callable, pure]
    public fun revertWithMessage() {
        abort_with(b"error message");
    }
}
