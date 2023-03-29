#[interface]
/// The interface for ERC-721.
/// This module defines the API for the interface of ERC-721 and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC721 {
    use Evm::Evm::{Unit};
    use Evm::Result::{Result};
    use Evm::U256::{U256};

    #[external]
    public native fun call_safeTransferFrom(contract: address, from: address, to: address, tokenId: U256): Result<Unit, vector<u8>>;

    #[external(name=safeTransferFrom)]
    public native fun call_safeTransferFrom_with_data(contract: address, from: address, to: address, tokenId: U256, data: vector<u8>): Result<Unit, vector<u8>>;

    #[interface_id]
    /// Return the interface identifier.
    // TODO: complete this function.
   public native fun interfaceId(): vector<u8>;

    // TODO: complete this module.
}
