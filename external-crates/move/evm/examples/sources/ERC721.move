#[evm_contract]
/// An implementation of the ERC-721 Non-Fungible Token Standard.
module Evm::ERC721 {
    use Evm::Evm::{sender, self, sign, emit, isContract, tokenURI_with_baseURI};
    use Evm::Result;
    use Evm::IERC721Receiver;
    use Evm::IERC721Metadata;
    use Evm::IERC721;
    use Evm::IERC165;
    use Evm::Table::{Self, Table};
    use Evm::U256::{Self, U256};
    use std::ascii::{Self, String};
    use std::errors;
    use std::vector;

    #[event]
    struct Transfer {
        from: address,
        to: address,
        tokenId: U256,
    }

    #[event]
    struct Approval {
        owner: address,
        approved: address,
        tokenId: U256,
    }

    #[event]
    struct ApprovalForAll {
        owner: address,
        operator: address,
        approved: bool,
    }

    #[storage]
    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        name: String,
        symbol: String,
        owners: Table<U256, address>,
        balances: Table<address, U256>,
        tokenApprovals: Table<U256, address>,
        operatorApprovals: Table<address, Table<address, bool>>,
    }

    #[create]
    /// Constructor of this contract.
    public fun create(name: String, symbol: String) {
        // Initial state of contract
        move_to<State>(
            &sign(self()),
            State {
                name,
                symbol,
                owners: Table::empty<U256, address>(),
                balances: Table::empty<address, U256>(),
                tokenApprovals: Table::empty<U256, address>(),
                operatorApprovals: Table::empty<address, Table<address, bool>>(),
            }
        );
    }

    #[callable]
    // Query if this contract implements a certain interface.
    public fun supportsInterface(interfaceId: vector<u8>): bool {
        &interfaceId == &IERC721::interfaceId() ||
            &interfaceId == &IERC721Metadata::interfaceId() ||
            &interfaceId == &IERC165::interfaceId()
    }

    #[callable]
    /// Get the name.
    public fun name(): String acquires State {
        let s = borrow_global<State>(self());
        *&s.name
    }

    #[callable]
    /// Get the symbol.
    public fun symbol(): String acquires State {
        let s = borrow_global<State>(self());
        *&s.symbol
    }

    #[callable]
    /// Get the name.
    public fun tokenURI(tokenId: U256): String {
        let baseURI = b""; // TODO: Implement this.
        ascii::string(tokenURI_with_baseURI(baseURI, tokenId))
    }

    #[callable]
    /// Count all NFTs assigned to an owner.
    public fun balanceOf(owner: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, owner)
    }

    #[callable]
    /// Find the owner of an NFT.
    public fun ownerOf(tokenId: U256): address acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_ownerOf(s, tokenId)
    }

    #[callable(name=safeTransferFrom)] // Overloading `safeTransferFrom`
    /// Transfers the ownership of an NFT from one address to another address.
    public fun safeTransferFrom_with_data(from: address, to: address, tokenId: U256, data: vector<u8>) acquires State {
        transferFrom(from, to, copy tokenId);
        doSafeTransferAcceptanceCheck(from, to, tokenId, data);

    }

    #[callable]
    /// Transfers the ownership of an NFT from one address to another address.
    public fun safeTransferFrom(from: address, to: address, tokenId: U256) acquires State {
        safeTransferFrom_with_data(from, to, tokenId, b"");
    }

    #[callable]
    /// Transfer ownership of an NFT. THE CALLER IS RESPONSIBLE
    ///  TO CONFIRM THAT `_to` IS CAPABLE OF RECEIVING NFTS OR ELSE
    ///  THEY MAY BE PERMANENTLY LOST
    public fun transferFrom(from: address, to: address, tokenId: U256) acquires State {
        assert!(isApprovedOrOwner(sender(), copy tokenId), errors::invalid_argument(0));
        assert!(ownerOf(copy tokenId) == from, errors::invalid_argument(0));
        assert!(to != @0x0, errors::invalid_argument(0));

        // Clear approvals from the previous owner
        approve(@0x0, copy tokenId);

        let s = borrow_global_mut<State>(self());

        let mut_balance_from = mut_balanceOf(s, from);
        *mut_balance_from = U256::sub(*mut_balance_from, U256::one());

        let mut_balance_to = mut_balanceOf(s, to);
        *mut_balance_to = U256::add(*mut_balance_to, U256::one());

        let mut_owner_token = mut_ownerOf(s, copy tokenId);
        *mut_owner_token = to;

        emit(Transfer{from, to, tokenId});
    }

    #[callable]
    /// Change or reaffirm the approved address for an NFT.
    public fun approve(approved: address, tokenId: U256) acquires State {
        let owner = ownerOf(copy tokenId);
        assert!(owner != @0x0, errors::invalid_argument(0));
        assert!(approved != owner, errors::invalid_argument(0));
        assert!(sender() == owner || isApprovedForAll(owner, sender()), errors::invalid_argument(0));

        let s = borrow_global_mut<State>(self());
        *mut_tokenApproval(s, copy tokenId) = approved;
        emit(Approval{ owner, approved, tokenId})
    }

    #[callable]
    /// Enable or disable approval for a third party ("operator") to manage
    ///  all of the sender's assets.
    public fun setApprovalForAll(operator: address, approved: bool) acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_operatorApproval(s, sender(), operator) = approved;
    }

    #[callable]
    /// Get the approved address for a single NFT.
    public fun getApproved(tokenId: U256): address acquires State {
        let s = borrow_global_mut<State>(self());
        assert!(tokenExists(s, copy tokenId), errors::invalid_argument(0));
        *mut_tokenApproval(s, tokenId)
    }

    #[callable]
    /// Query if an address is an authorized operator for another address.
    public fun isApprovedForAll(owner: address, operator: address): bool acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_operatorApproval(s, owner, operator)
    }

    /// Helper function to return true iff `spender` is the owner or an approved one for `tokenId`.
    fun isApprovedOrOwner(spender: address, tokenId: U256): bool acquires State {
        let s = borrow_global_mut<State>(self());
        assert!(tokenExists(s, copy tokenId), errors::invalid_argument(0));
        let owner = ownerOf(copy tokenId);
        return (spender == owner || getApproved(tokenId) == spender || isApprovedForAll(owner, spender))
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_balanceOf(s: &mut State, owner: address): &mut U256 {
        Table::borrow_mut_with_default(&mut s.balances, &owner, U256::zero())
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_ownerOf(s: &mut State, tokenId: U256): &mut address {
        Table::borrow_mut_with_default(&mut s.owners, &tokenId, @0x0)
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_tokenApproval(s: &mut State, tokenId: U256): &mut address {
        Table::borrow_mut_with_default(&mut s.tokenApprovals, &tokenId, @0x0)
    }

    /// Helper function to return a mut ref to the operator approval.
    fun mut_operatorApproval(s: &mut State, owner: address, operator: address): &mut bool {
        if(!Table::contains(&s.operatorApprovals, &owner)) {
            Table::insert(
                &mut s.operatorApprovals,
                &owner,
                Table::empty<address, bool>()
            )
        };
        let approvals = Table::borrow_mut(&mut s.operatorApprovals, &owner);
        Table::borrow_mut_with_default(approvals, &operator, false)
    }

    /// Helper function to return true iff the token exists.
    fun tokenExists(s: &mut State, tokenId: U256): bool {
        let mut_ownerOf_tokenId = mut_ownerOf(s, tokenId);
        *mut_ownerOf_tokenId != @0x0
    }

    /// Helper function for the acceptance check.
    fun doSafeTransferAcceptanceCheck(from: address, to: address, tokenId: U256, data: vector<u8>) {
        if (isContract(to)) {
            let result = IERC721Receiver::try_call_onERC721Received(to, sender(), from, tokenId, data);
            if (Result::is_ok(&result)) {
                let retval = Result::unwrap(result);
                let expected = IERC721Receiver::selector_onERC721Received();
                assert!(retval == expected, errors::custom(0));
            }
            else {
                let error_reason = Result::unwrap_err(result);
                if(vector::length(&error_reason) == 0) {
                    abort(errors::custom(1)) // ERC721: transfer to non ERC721Receiver implementer
                }
                else {
                    abort(errors::custom(2)) // TODO: abort with the `_error` value.
                }
            }
        }
    }
}
