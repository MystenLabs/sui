#[evm_contract]
module 0x1::Native {
    use Evm::Evm::{self, sender, isContract};

    #[callable, view]
    public fun getContractAddr(): address {
        self()
    }

    #[callable, view]
    public fun getSenderAddr(): address {
        sender()
    }

    #[callable, view]
    public fun getIsContract(a: address): bool {
        isContract(a)
    }
}
