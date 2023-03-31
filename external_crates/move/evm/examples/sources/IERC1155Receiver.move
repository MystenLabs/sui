#[interface]
/// The interface for ERC-721 Token Receiver.
/// This module defines the API for the interface of ERC-721 Token Receiver and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC1155Receiver {
    use Evm::Evm::{keccak256, bytes4, bytes_xor};
    use Evm::Result::{Result};
    use Evm::U256::{U256};

    #[external]
    public native fun call_onERC1155Received(contract: address, operator: address, from: address, id: U256, amount: U256, bytes: vector<u8>): Result<vector<u8>, vector<u8>>;

    #[external]
    public native fun call_onERC1155BatchReceived(contract: address, operator: address, from: address, ids: vector<U256>, amounts: vector<U256>, bytes: vector<u8>): Result<vector<u8>, vector<u8>>;

    #[selector]
    /// Return the selector of the function `onERC1155Received`
    public fun selector_onERC1155Received(): vector<u8> {
        bytes4(keccak256(b"onERC1155Received(address,address,uint256,uint256,bytes)"))
    }

    #[selector]
    /// Return the selector of the function `onERC1155Received`
    public fun selector_onERC1155BatchReceived(): vector<u8> {
        bytes4(keccak256(b"onERC1155BatchReceived(address,address,uint256[],uint256[],bytes)"))
    }

    #[interface_id]
    /// Return the interface identifier of this interface.
    public fun interfaceId(): vector<u8> {
        bytes_xor(
            selector_onERC1155Received(),
            selector_onERC1155BatchReceived()
        )
    }
}
