#[evm_contract]
/// An implementation of the ERC-721 Non-Fungible Token Standard.
module 0x2::ERC721 {
    use Evm::U256::{U256, u256_from_words};


    #[callable(sig=b"balanceOf(address) returns (uint256)")]
    /// Count all NFTs assigned to an owner.
    public fun balanceOf(_owner: address): U256 {
        u256_from_words(0,0)
    }

    #[callable(sig=b"ownerOf(uint256) returns (address)")]
    /// Find the owner of an NFT.
    public fun ownerOf(_tokenId: U256): address {
        @0x1
    }

    #[callable(sig=b"safeTransferFrom(address,address,uint256, bytes)")] // Overloading `safeTransferFrom`
    /// Transfers the ownership of an NFT from one address to another address.
    public fun safeTransferFrom_with_data(_from: address, _to: address, _tokenId: U256, _data: vector<u8>) {
    }

    #[callable(sig=b"safeTransferFrom(address,address,uint256)")]
    /// Transfers the ownership of an NFT from one address to another address.
    public fun safeTransferFrom(from: address, to: address, tokenId: U256) {
        safeTransferFrom_with_data(from, to, tokenId, b"");
    }

    #[callable(sig=b"transferFrom(address,address,uint256)")]
    /// Transfer ownership of an NFT. THE CALLER IS RESPONSIBLE
    ///  TO CONFIRM THAT `_to` IS CAPABLE OF RECEIVING NFTS OR ELSE
    ///  THEY MAY BE PERMANENTLY LOST
    public fun transferFrom(_from: address, _to: address, _tokenId: U256) {
    }

    #[callable(sig=b"approve(address, uint256)")]
    /// Change or reaffirm the approved address for an NFT.
    public fun approve(_approved: address, _tokenId: U256){
    }

    #[callable(sig=b"setApprovalForAll(address,bool)")]
    /// Enable or disable approval for a third party ("operator") to manage all of the caller's tokens.
    public fun setApprovalForAll(_operator: address, _approved: bool) {
    }

    #[callable(sig=b"getApproved(uint256) returns (address)")]
    /// Get the approved address for a single NFT.
    public fun getApproved(_tokenId: U256): address {
        @0x1
    }

    #[callable(sig=b"isApprovalForAll(address,address)returns(bool)"), view]
    /// Queries the approval status of an operator for a given owner.
    public fun isApprovalForAll(_account: address, _operator: address): bool {
        true
    }

    #[callable(sig=b"supportsInterface(bytes4) returns (bool)")]
    // Query if this contract implements a certain interface.
    public fun supportsInterface(_interfaceId: vector<u8>): bool {
        true
    }
}
