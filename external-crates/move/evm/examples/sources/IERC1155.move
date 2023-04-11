#[interface]
/// The interface for ERC-1155.
/// This module defines the API for the interface of ERC-1155 and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC1155 {

    #[interface_id]
    /// Return the interface identifier.
    // TODO: complete this function.
   public native fun interfaceId(): vector<u8>;

    // TODO: complete this module.
}
