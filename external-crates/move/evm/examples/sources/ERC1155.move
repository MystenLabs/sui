#[evm_contract]
/// An implementation of the ERC-1155 Multi Token Standard.
module Evm::ERC1155 {
    use Evm::Evm::{sender, self, sign, emit, isContract};
    use Evm::IERC1155Receiver;
    use Evm::IERC165;
    use Evm::IERC1155;
    use Evm::Table::{Self, Table};
    use Evm::Result;
    use Evm::U256::{Self, U256};
    use std::ascii::{String};
    use std::errors;
    use std::vector;

    #[event]
    struct TransferSingle {
        operator: address,
        from: address,
        to: address,
        id: U256,
        value: U256,
    }

    #[event]
    struct TransferBatch {
        operator: address,
        from: address,
        to: address,
        ids: vector<U256>,
        values: vector<U256>,
    }

    #[event]
    struct ApprovalForAll {
        owner: address,
        operator: address,
        approved: bool,
    }

    #[event]
    struct URI {
        value: String,
        id: U256,
    }

    #[storage]
    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        balances: Table<U256, Table<address, U256>>,
        operatorApprovals: Table<address, Table<address, bool>>,
        uri: String,
        owner: address, // Implements the "ownable" pattern.
    }

    #[create]
    /// Constructor of this contract.
    public fun create(uri: String) {
        // Initial state of contract
        move_to<State>(
            &sign(self()),
            State {
                balances: Table::empty<U256, Table<address, U256>>(),
                operatorApprovals: Table::empty<address, Table<address, bool>>(),
                uri,
                owner: sender(),
            }
        );
    }

    #[callable, view]
    /// Returns the name of the token
    public fun uri(): String acquires State {
        *&borrow_global<State>(self()).uri
    }

    #[callable, view]
    /// Get the balance of an account's token.
    public fun balanceOf(account: address, id: U256): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, id, account)
    }

    #[callable, view]
    /// Get the balance of multiple account/token pairs.
    public fun balanceOfBatch(accounts: vector<address>, ids: vector<U256>): vector<U256> acquires State {
        assert!(vector::length(&accounts) == vector::length(&ids), errors::invalid_argument(0));
        let len = vector::length(&accounts);
        let i = 0;
        let balances = vector::empty<U256>();
        while(i < len) {
            vector::push_back(
                &mut balances,
                balanceOf(
                    *vector::borrow(&accounts, i),
                    *vector::borrow(&ids, i)
                )
            );
            i = i + 1;
        };
        balances
    }

    #[callable, view]
    /// Enable or disable approval for a third party ("operator") to manage all of the caller's tokens.
    public fun setApprovalForAll(operator: address, approved: bool) acquires State {
        let s = borrow_global_mut<State>(self());
        let operatorApproval = mut_operatorApprovals(s, sender(), operator);
        *operatorApproval = approved;
        emit(ApprovalForAll{owner: sender(), operator, approved});
    }

    #[callable, view]
    /// Queries the approval status of an operator for a given owner.
    public fun isApprovalForAll(account: address, operator: address): bool acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_operatorApprovals(s, account, operator)
    }

    #[callable]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeTransferFrom(from: address, to: address, id: U256, amount: U256, _data: vector<u8>) acquires State {
        assert!(from == sender() || isApprovalForAll(from, sender()), errors::invalid_argument(0));
        let s = borrow_global_mut<State>(self());
        let mut_balance_from = mut_balanceOf(s, copy id, from);
        assert!(U256::le(copy amount, *mut_balance_from), errors::invalid_argument(0));
        *mut_balance_from = U256::sub(*mut_balance_from, copy amount);
        let mut_balance_to = mut_balanceOf(s, copy id, to);
        *mut_balance_to = U256::add(*mut_balance_to, copy amount);
        // TODO: Unit testing does not support events yet.
        //let operator = sender();
        //emit(TransferSingle{operator, from, to, id: copy id, value: copy amount});
        // TODO: Unit testing does not support the following function.
        //doSafeTransferAcceptanceCheck(operator, from, to, id, amount, data);
    }

    #[callable]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeBatchTransferFrom(from: address, to: address, ids: vector<U256>, amounts: vector<U256>, data: vector<u8>) acquires State {
        assert!(from == sender() || isApprovalForAll(from, sender()), errors::invalid_argument(0));
        assert!(vector::length(&amounts) == vector::length(&ids), errors::invalid_argument(0));
        let len = vector::length(&amounts);
        let i = 0;

        let operator = sender();
        let s = borrow_global_mut<State>(self());

        while(i < len) {
            let id = *vector::borrow(&ids, i);
            let amount = *vector::borrow(&amounts, i);

            let mut_balance_from = mut_balanceOf(s, copy id, from);
            assert!(U256::le(copy amount, *mut_balance_from), errors::invalid_argument(0));
            *mut_balance_from = U256::sub(*mut_balance_from, copy amount);
            let mut_balance_to = mut_balanceOf(s, id, to);
            *mut_balance_to = U256::add(*mut_balance_to, amount);

            i = i + 1;
        };

        emit(TransferBatch{operator, from, to, ids: copy ids, values: copy amounts});

        doSafeBatchTransferAcceptanceCheck(operator, from, to, ids, amounts, data);
    }

    #[callable]
    // Query if this contract implements a certain interface.
    public fun supportsInterface(interfaceId: vector<u8>): bool {
        &interfaceId == &IERC1155::interfaceId() || &interfaceId == &IERC165::interfaceId()
    }

    #[callable]
    public fun owner(): address acquires State {
        borrow_global_mut<State>(self()).owner
    }

    #[callable]
    // Query if this contract implements a certain interface.
    public fun mint(to: address, id: U256, amount: U256, _data: vector<u8>) acquires State {
        assert!(sender() == owner(), errors::invalid_argument(0)); // Only owner can mint.
        let s = borrow_global_mut<State>(self());
        let mut_balance_to = mut_balanceOf(s, copy id, to);
        *mut_balance_to = U256::add(*mut_balance_to, copy amount);
        // TODO: Unit testing does not support events yet.
        //emit(TransferSingle{operator: sender(), from: @0x0, to, id: copy id, value: copy amount});
    }

    #[callable]
    // Query if this contract implements a certain interface.
    public fun mintBatch(to: address, ids: vector<U256>, amounts: vector<U256>, _data: vector<u8>) acquires State {
        assert!(sender() == owner(), errors::invalid_argument(0)); // Only owner can mint.
        assert!(vector::length(&amounts) == vector::length(&ids), errors::invalid_argument(0));
        let len = vector::length(&amounts);
        let i = 0;

        let s = borrow_global_mut<State>(self());

        while(i < len) {
            let id = *vector::borrow(&ids, i);
            let amount = *vector::borrow(&amounts, i);

            let mut_balance_to = mut_balanceOf(s, id, to);
            *mut_balance_to = U256::add(*mut_balance_to, amount);

            i = i + 1;
        };
        // TODO: Unit testing does not support events yet.
        //emit(TransferBatch{operator: sender(), from: @0x0, to, ids: copy ids, values: copy amounts});
    }

    /// Helper function to return a mut ref to the operatorApproval
    fun mut_operatorApprovals(s: &mut State, account: address, operator: address): &mut bool {
        if(!Table::contains(&s.operatorApprovals, &account)) {
            Table::insert(
                &mut s.operatorApprovals,
                &account,
                Table::empty<address, bool>()
            )
        };
        let operatorApproval_account = Table::borrow_mut(
            &mut s.operatorApprovals,
            &account
        );
        Table::borrow_mut_with_default(operatorApproval_account, &operator, false)
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_balanceOf(s: &mut State, id: U256, account: address): &mut U256 {
        if(!Table::contains(&s.balances, &id)) {
            Table::insert(
                &mut s.balances,
                &id,
                Table::empty<address, U256>()
            )
        };
        let balances_id = Table::borrow_mut(&mut s.balances, &id);
        Table::borrow_mut_with_default(balances_id, &account, U256::zero())
    }

    /// Helper function for the safe transfer acceptance check.
    fun doSafeTransferAcceptanceCheck(operator: address, from: address, to: address, id: U256, amount: U256, data: vector<u8>) {
        if (isContract(to)) {
            let result = IERC1155Receiver::call_onERC1155Received(to, operator, from, id, amount, data);
            if (Result::is_ok(&result)) {
                let retval = Result::unwrap(result);
                let expected = IERC1155Receiver::selector_onERC1155Received();
                assert!(retval == expected, errors::custom(0));
            }
            else {
                let _error = Result::unwrap_err(result);
                abort(errors::custom(1)) // TODO: abort with the `_error` value.
            }
        }
    }

    /// Helper function for the safe batch transfer acceptance check.
    fun doSafeBatchTransferAcceptanceCheck(operator: address, from: address, to: address, ids: vector<U256>, amounts: vector<U256>, data: vector<u8>) {
        if (isContract(to)) {
            let result = IERC1155Receiver::call_onERC1155BatchReceived(to, operator, from, ids, amounts, data);
            if (Result::is_ok(&result)) {
                let retval = Result::unwrap(result);
                let expected = IERC1155Receiver::selector_onERC1155BatchReceived();
                assert!(retval == expected, errors::custom(0));
            }
            else {
                let _error = Result::unwrap_err(result);
                abort(errors::custom(1)) // TODO: abort with the `_error` value.
            }
        }
    }
}
