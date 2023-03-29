#[evm_contract]
/// An implementation of the ERC-165.
module Evm::ERC165 {
    use Evm::IERC165;
    use std::errors;
    use std::vector;

    // Query if a contract implements an interface.
    // The length of `interfaceId` is required to be 4.
    public fun supportInterface(interfaceId: vector<u8>): bool {
        assert!(vector::length(&interfaceId) == 4, errors::invalid_argument(0));
        (interfaceId == IERC165::selector_supportInterface())
    }
}
