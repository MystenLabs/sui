#[interface]
/// The interface for ERC-721 Token Receiver.
/// This module defines the API for the interface of ERC-721 Token Receiver and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC721Receiver {
    use Evm::Evm::{keccak256, bytes4};
    use Evm::Result::{Result};
    use Evm::U256::{U256};

    #[external]
    public native fun call_onERC721Received(contract: address, operator: address, from: address, tokenId: U256, bytes: vector<u8>): vector<u8>;

    #[external]
    public native fun try_call_onERC721Received(contract: address, operator: address, from: address, tokenId: U256, bytes: vector<u8>): Result<vector<u8>, vector<u8>>;

    #[selector]
    /// Return the selector of the function `onERC721Received`
    public fun selector_onERC721Received(): vector<u8> {
        bytes4(keccak256(b"onERC721Received(address,address,uint256,bytes)"))
    }

    #[interface_id]
    /// Return the interface identifier for this interface.
    public fun interfaceId(): vector<u8> {
        selector_onERC721Received()
    }
}
