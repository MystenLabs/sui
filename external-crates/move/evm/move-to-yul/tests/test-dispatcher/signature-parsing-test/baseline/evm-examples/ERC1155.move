#[evm_contract]
/// An implementation of the ERC-1155 Multi Token Standard.
module 0x2::ERC1155 {
    use std::ascii::{String};
    use Evm::U256::{U256, u256_from_words};

    /*
    #[callable(sig=b"uri() returns (string memory) "), view]
    /// Returns the name of the token
    public fun uri(): String {
        std::ascii::String(b"abc") // TODO: this leads to compilation error "type needs to be struct or vector"
    }
    */


    #[callable(sig=b"balanceOf(address,uint256) returns (uint256)"), view]
    /// Get the balance of an account's token.
    public fun balanceOf(_account: address, _id: U256): U256 {
        u256_from_words(0,0)
    }

    #[callable(sig=b"balanceOfBatch(address[], uint256[]) returns (uint256[] memory)"), view]
    /// Get the balance of multiple account/token pairs.
    public fun balanceOfBatch(_accounts: vector<address>, ids: vector<U256>): vector<U256> {
        ids
    }

    #[callable(sig=b"setApprovalForAll(address,bool)")]
    /// Enable or disable approval for a third party ("operator") to manage all of the caller's tokens.
    public fun setApprovalForAll(_operator: address, _approved: bool) {
    }

    #[callable(sig=b"isApprovalForAll(address,address)returns(bool)"), view]
    /// Queries the approval status of an operator for a given owner.
    public fun isApprovalForAll(_account: address, _operator: address): bool {
        true
    }

    #[callable(sig=b"safeTransferFrom(address,address,uint256,uint256,bytes)")]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeTransferFrom(_from: address, _to: address, _id: U256, _amount: U256, _data: vector<u8>) {
    }

    #[callable(sig=b"safeBatchTransferFrom(address,address,uint256[],uint256[],bytes)")]
    /// Transfers `_value` amount of an `_id` from the `_from` address to the `_to` address specified (with safety call).
    public fun safeBatchTransferFrom(_from: address, _to: address, _ids: vector<U256>, _amounts: vector<U256>, _data: vector<u8>) {
    }

    #[callable(sig=b"supportsInterface(bytes4) returns (bool)")]
    // Query if this contract implements a certain interface.
    public fun supportsInterface(_interfaceId: vector<u8>): bool {
        true
    }
}
