#[interface]
/// The interface for the ERC-165.
/// This module defines the API for the interface of ERC-165 and
/// the utility functions such as selectors and `interfaceId`.
module Evm::IERC165 {
    use Evm::Evm::{keccak256, bytes4};
    use Evm::Result::{Result};

    #[external]
    public native fun call_supportInterface(contract: address, interfaceId: vector<u8>): Result<bool, vector<u8>>;

    #[selector]
    public fun selector_supportInterface(): vector<u8> {
        bytes4(keccak256(b"supportInterface(bytes4)"))
    }

    #[interface_id]
    /// Return the interface identifier. This function corresponds to
    /// `type(I).interfaceId` in Solidity where `I` is an interface.
    // The following is a excerpt from the Solidity documentation:
    //   A bytes4 value containing the EIP-165 interface identifier of
    //   the given interface I. This identifier is defined as the XOR
    //   of all function selectors defined within the interface itself
    //   - excluding all inherited functions.
    public fun interfaceId(): vector<u8> {
        selector_supportInterface()
    }
}
