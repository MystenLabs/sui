#[interface]
/// The interface for ERC-721 Metadata.
/// This module defines the API for the interface of ERC-721 Metadata and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC721Metadata {
    use Evm::Evm::{bytes_xor, bytes4, keccak256};
    use Evm::Result::{Result};
    use std::ascii::{String};
    use Evm::U256::{U256};

    #[external]
    public native fun call_name(contract: address): Result<String, vector<u8>>;

    #[external]
    public native fun call_symbol(contract: address): Result<String, vector<u8>>;

    #[external]
    public native fun call_tokenURI(contract: address, tokenId: U256): Result<String, vector<u8>>;

    #[selector]
    public fun selector_name(): vector<u8> {
        bytes4(keccak256(b"name()"))
    }

    #[selector]
    public fun selector_symbol(): vector<u8> {
        bytes4(keccak256(b"symbol()"))
    }

    #[selector]
    public fun selector_tokenURI(): vector<u8> {
        bytes4(keccak256(b"tokenURI(uint256)"))
    }

    #[interface_id]
    /// Return the interface identifier.
    public fun interfaceId(): vector<u8> {
        bytes_xor(
            bytes_xor(
                selector_name(),
                selector_symbol()
            ),
            selector_tokenURI()
        )
   }
}
