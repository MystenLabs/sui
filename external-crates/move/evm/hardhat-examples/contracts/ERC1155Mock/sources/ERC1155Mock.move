#[evm_contract]
/// An implementation of the ERC-1155 Multi Token Standard.
module Evm::ERC1155Mock {
    use Evm::Evm::{sender, self, sign, emit, isContract, abort_with, require};
    use Evm::Table::{Self, Table};
    use Evm::ExternalResult::{Self, ExternalResult};
    use Evm::U256::{Self, U256};
    use std::vector;

    // ---------------------
    // For test only
    // ---------------------

    #[callable(sig=b"setURI(string)")]
    public fun setURI(newuri: vector<u8>) acquires State {
        borrow_global_mut<State>(self()).uri = newuri;
    }

    #[callable(sig=b"mint(address,uint256,uint256,bytes)")]
    public fun mint(to: address, id: U256, amount: U256, data: vector<u8>) acquires State {
        mint_(to, id, amount, data);
    }

    #[callable(sig=b"mintBatch(address,uint256[],uint256[],bytes)")]
    public fun mintBatch(to: address, ids: vector<U256>, amounts: vector<U256>, data: vector<u8>) acquires State {
        mintBatch_(to, ids, amounts, data);
    }

    #[callable(sig=b"burn(address,uint256,uint256)")]
    public fun burn(owner: address, id: U256, amount: U256) acquires State {
        burn_(owner, id, amount);
    }

    #[callable(sig=b"burnBatch(address,uint256[],uint256[])")]
    public fun burnBatch(owner: address, ids: vector<U256>, amounts: vector<U256>) acquires State {
        burnBatch_(owner, ids, amounts);
    }


    // ---------------------
    // Evm::IERC1155Receiver
    // ---------------------

    #[external(sig=b"onERC1155Received(address,address,uint256,uint256,bytes) returns (bytes4)")]
    public native fun IERC1155Receiver_try_call_onERC1155Received(contract: address, operator: address, from: address, id: U256, amount: U256, bytes: vector<u8>): ExternalResult<vector<u8>>;

    #[external(sig=b"onERC1155BatchReceived(address,address,uint256[],uint256[],bytes) returns (bytes4)")]
    public native fun IERC1155Receiver_try_call_onERC1155BatchReceived(contract: address, operator: address, from: address, ids: vector<U256>, amounts: vector<U256>, bytes: vector<u8>): ExternalResult<vector<u8>>;

    /// Return the selector of the function `onERC1155Received`
    public fun IERC1155Receiver_selector_onERC1155Received(): vector<u8> {
        //bytes4(keccak256(b"onERC1155Received(address,address,uint256,uint256,bytes)"))
        x"f23a6e61"
    }

    /// Return the selector of the function `onERC1155Received`
    public fun IERC1155Receiver_selector_onERC1155BatchReceived(): vector<u8> {
        //bytes4(keccak256(b"onERC1155BatchReceived(address,address,uint256[],uint256[],bytes)"))
        x"bc197c81"
    }

    /// Return the interface identifier of this interface.
    public fun IERC1155Receiver_interfaceId(): vector<u8> {
        // TODO: Eager evaulate this at the compile time for optimization.
        // bytes_xor(
        //     IERC1155Receiver_selector_onERC1155Received(),
        //     IERC1155Receiver_selector_onERC1155BatchReceived()
        // )
        //x"4e2312e0"
        x"4e2312e1" // TODO: wrong value
    }

    // ---------------------
    // Evm::IERC165
    // ---------------------
    public fun IERC165_interfaceId(): vector<u8> {
        // TODO: Eager evaulate this at the compile time for optimization.
        //bytes4(keccak256(b"supportsInterface(bytes4)"))
        x"01ffc9a7"
    }

    // ---------------------
    // Evm::IERC1155
    // ---------------------
    public fun IERC1155_interfaceId(): vector<u8> {
        // TODO: Eager evaulate this at the compile time for optimization.
        //bytes4(keccak256(b"supportsInterface(bytes4)"))
        x"d9b67a26"
    }

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
        account: address,
        operator: address,
        approved: bool,
    }

    #[event]
    struct URI {
        value: vector<u8>,
        id: U256,
    }

    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        balances: Table<U256, Table<address, U256>>,
        operatorApprovals: Table<address, Table<address, bool>>,
        uri: vector<u8>,
        owner: address, // Implements the "ownable" pattern.
    }

    #[create(sig=b"constructor(string)")]
    /// Constructor of this contract.
    public fun create(uri: vector<u8>) {
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

    #[callable(sig=b"uri(uint256) returns (string)"), view]
    /// Returns the name of the token
    public fun uri(_id: U256): vector<u8> acquires State {
        *&borrow_global<State>(self()).uri
    }

    #[callable(sig=b"balanceOf(address,uint256) returns (uint256)"), view]
    /// Get the balance of an account's token.
    public fun balanceOf(account: address, id: U256): U256 acquires State {
        require(account != @0x0, b"ERC1155: balance query for the zero address");
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, id, account)
    }

    #[callable(sig=b"balanceOfBatch(address[],uint256[]) returns (uint256[])"), view]
    /// Get the balance of multiple account/token pairs.
    public fun balanceOfBatch(accounts: vector<address>, ids: vector<U256>): vector<U256> acquires State {
        require(vector::length(&accounts) == vector::length(&ids), b"ERC1155: accounts and ids length mismatch");
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

    #[callable(sig=b"setApprovalForAll(address,bool)")]
    /// Enable or disable approval for a third party ("operator") to manage all of the caller's tokens.
    public fun setApprovalForAll(operator: address, approved: bool) acquires State {
        let owner = sender();
        require(owner != operator, b"ERC1155: setting approval status for self");
        let s = borrow_global_mut<State>(self());
        let operatorApproval = mut_operatorApprovals(s, owner, operator);
        *operatorApproval = approved;
        emit(ApprovalForAll{account: owner, operator, approved});
    }

    #[callable(sig=b"isApprovedForAll(address,address) returns (bool)"), view]
    /// Queries the approval status of an operator for a given owner.
    public fun isApprovedForAll(account: address, operator: address): bool acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_operatorApprovals(s, account, operator)
    }

    #[callable(sig=b"safeTransferFrom(address,address,uint256,uint256,bytes)")]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeTransferFrom(from: address, to: address, id: U256, amount: U256, data: vector<u8>) acquires State {
        require(to != @0x0, b"ERC1155: transfer to the zero address");
        require(from == sender() || isApprovedForAll(from, sender()), b"ERC1155: caller is not owner nor approved");
        let s = borrow_global_mut<State>(self());
        let mut_balance_from = mut_balanceOf(s, copy id, from);
        require(U256::le(copy amount, *mut_balance_from), b"ERC1155: insufficient balance for transfer");
        *mut_balance_from = U256::sub(*mut_balance_from, copy amount);
        let mut_balance_to = mut_balanceOf(s, copy id, to);
        *mut_balance_to = U256::add(*mut_balance_to, copy amount);
        let operator = sender();

        emit(TransferSingle{operator, from, to, id: copy id, value: copy amount});

        doSafeTransferAcceptanceCheck(operator, from, to, id, amount, data);
    }

    #[callable(sig=b"safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)")]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeBatchTransferFrom(from: address, to: address, ids: vector<U256>, amounts: vector<U256>, data: vector<u8>) acquires State {
        require(to != @0x0, b"ERC1155: transfer to the zero address");
        require(from == sender() || isApprovedForAll(from, sender()), b"ERC1155: transfer caller is not owner nor approved");
        require(vector::length(&amounts) == vector::length(&ids), b"ERC1155: ids and amounts length mismatch");
        let len = vector::length(&amounts);
        let i = 0;

        let operator = sender();
        let s = borrow_global_mut<State>(self());

        while(i < len) {
            let id = *vector::borrow(&ids, i);
            let amount = *vector::borrow(&amounts, i);

            let mut_balance_from = mut_balanceOf(s, copy id, from);
            require(U256::le(copy amount, *mut_balance_from), b"ERC1155: insufficient balance for transfer");
            *mut_balance_from = U256::sub(*mut_balance_from, copy amount);
            let mut_balance_to = mut_balanceOf(s, id, to);
            *mut_balance_to = U256::add(*mut_balance_to, amount);

            i = i + 1;
        };

        emit(TransferBatch{operator, from, to, ids: copy ids, values: copy amounts});

        doSafeBatchTransferAcceptanceCheck(operator, from, to, ids, amounts, data);
    }

    #[callable(sig=b"supportsInterface(bytes4) returns (bool)"), view]
    // Query if this contract implements a certain interface.
    public fun supportsInterface(interfaceId: vector<u8>): bool {
        interfaceId == IERC165_interfaceId() || interfaceId == IERC1155_interfaceId()
    }

    #[callable(sig=b"owner() returns (address)"), view]
    public fun owner(): address acquires State {
        borrow_global_mut<State>(self()).owner
    }

    // Internal function for minting.
    fun mint_(to: address, id: U256, amount: U256, _data: vector<u8>) acquires State {
        require(to != @0x0, b"ERC1155: mint to the zero address");
        let s = borrow_global_mut<State>(self());
        let mut_balance_to = mut_balanceOf(s, copy id, to);
        *mut_balance_to = U256::add(*mut_balance_to, copy amount);
        emit(TransferSingle{operator: sender(), from: @0x0, to, id: copy id, value: copy amount});
    }

    /// Internal function for mintBatch
    fun mintBatch_(to: address, ids: vector<U256>, amounts: vector<U256>, _data: vector<u8>) acquires State {
        require(to != @0x0, b"ERC1155: mint to the zero address");
        require(vector::length(&amounts) == vector::length(&ids), b"ERC1155: ids and amounts length mismatch");
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
        emit(TransferBatch{operator: sender(), from: @0x0, to, ids: copy ids, values: copy amounts});
    }

    public fun burn_(owner: address, id: U256, amount: U256) acquires State {
        require(owner != @0x0, b"ERC1155: burn from the zero address");
        let s = borrow_global_mut<State>(self());
        let mut_balance_owner = mut_balanceOf(s, id, owner);
        require(U256::ge(*mut_balance_owner, amount), b"ERC1155: burn amount exceeds balance");
        *mut_balance_owner = U256::sub(*mut_balance_owner, amount);
        emit(TransferSingle{operator: sender(), from: owner, to: @0x0, id, value: amount});
    }

    public fun burnBatch_(owner: address, ids: vector<U256>, amounts: vector<U256>) acquires State {
        require(owner != @0x0, b"ERC1155: burn from the zero address");
        require(vector::length(&amounts) == vector::length(&ids), b"ERC1155: ids and amounts length mismatch");
        let len = vector::length(&amounts);
        let i = 0;
        let s = borrow_global_mut<State>(self());
        while(i < len) {
            let id = *vector::borrow(&ids, i);
            let amount = *vector::borrow(&amounts, i);

            let mut_balance_owner = mut_balanceOf(s, id, owner);
            require(U256::ge(*mut_balance_owner, amount), b"ERC1155: burn amount exceeds balance");
            *mut_balance_owner = U256::sub(*mut_balance_owner, amount);

            i = i + 1;
        };
        emit(TransferBatch{operator: sender(), from: owner, to: @0x0, ids, values: amounts});
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
            let result = IERC1155Receiver_try_call_onERC1155Received(to, operator, from, id, amount, data);
            if (ExternalResult::is_err_reason(&result)) {
                // abort_with(b"err_reason");
                let reason = ExternalResult::unwrap_err_reason(result);
                abort_with(reason);
            } else if (ExternalResult::is_err_data(&result)) {
                abort_with(b"ERC1155: transfer to non ERC1155Receiver implementer");
            } else if (ExternalResult::is_panic(&result)) {
                abort_with(b"panic");
            } else if (ExternalResult::is_ok(&result)) {
                // abort_with(b"ok");
                let retval = ExternalResult::unwrap(result);
                let expected = IERC1155Receiver_selector_onERC1155Received();
                require(retval == expected, b"ERC1155: ERC1155Receiver rejected tokens");
            } else {
                abort_with(b"other");
            }
        }
    }

    /// Helper function for the safe batch transfer acceptance check.
    fun doSafeBatchTransferAcceptanceCheck(operator: address, from: address, to: address, ids: vector<U256>, amounts: vector<U256>, data: vector<u8>) {
        if (isContract(to)) {
            let result = IERC1155Receiver_try_call_onERC1155BatchReceived(to, operator, from, ids, amounts, data);
            if (ExternalResult::is_err_reason(&result)) {
                // abort_with(b"err_reason");
                let reason = ExternalResult::unwrap_err_reason(result);
                abort_with(reason);
            } else if (ExternalResult::is_err_data(&result)) {
                abort_with(b"ERC1155: transfer to non ERC1155Receiver implementer");
            } else if (ExternalResult::is_panic(&result)) {
                abort_with(b"panic");
            } else if (ExternalResult::is_ok(&result)) {
                // abort_with(b"ok");
                let retval = ExternalResult::unwrap(result);
                let expected = IERC1155Receiver_selector_onERC1155BatchReceived();
                require(retval == expected, b"ERC1155: ERC1155Receiver rejected tokens");
            } else {
                abort_with(b"other");
            }
        }
    }
}
