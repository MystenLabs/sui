#[evm_contract]
/// Faucet example in the Ethereum book.
module 0x42::Faucet {
    use Evm::Evm::{sender, value, self, sign, balance, transfer, emit};
    use Evm::U256::{Self, U256};
    use std::errors;

    #[storage]
    struct State has key {
        owner: address,
    }

    #[event]
    struct WithdrawalEvent {
        to: address,
        amount: U256
    }

    #[event]
    struct DepositEvent {
        from: address,
        amount: U256
    }

    #[create]
    public fun create() {
        move_to<State>(&sign(self()), State{owner: sender()})
    }

    #[delete]
    public fun delete() acquires State {
        let state = borrow_global<State>(self());
        assert!(sender() == state.owner, errors::requires_address(0));
    }

    #[receive, payable]
    public fun receive() {
        emit(DepositEvent{from: sender(), amount: value()})
    }

    #[callable]
    public fun withdraw(amount: U256) acquires State {
        let state = borrow_global<State>(self());

        // Don't allow to withdraw from self.
        assert!(state.owner != self(), errors::invalid_argument(0));

        // Limit withdrawal amount
        assert!(U256::le(copy amount, U256::u256_from_u128(100)), errors::invalid_argument(0));

        // Funds must be available.
        assert!(U256::le(copy amount, balance(self())), errors::limit_exceeded(0));

        // Transfer funds
        transfer(sender(), copy amount);
        emit(WithdrawalEvent{to: sender(), amount})
    }
}
