pragma solidity ^0.8.20;

// These contracts are for testing only, they are not safe for use in production.

interface IERC1822Proxiable {
    function proxiableUUID() external view returns (bytes32);
}

contract Proxiable {
    bytes32 internal constant _IMPLEMENTATION_SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    string public constant UPGRADE_INTERFACE_VERSION = "5.0.0";

    function upgradeToAndCall(address newImplementation, bytes calldata data) external {
        try IERC1822Proxiable(newImplementation).proxiableUUID() returns (bytes32 slot) {
            if (slot != _IMPLEMENTATION_SLOT) {
                revert("slot is unsupported as a uuid");
            }
            _setImplementation(newImplementation);
            if (data.length > 0) {
                /**
                 * Note that using delegate call can make your implementation contract vulnerable if this function
                 * is not protected with the `onlyProxy` modifier. Again, this contract is for testing only, it is
                 * not safe for use in production. Instead, use the `UUPSUpgradeable` contract available in
                 * @openzeppelin/contracts-upgradeable
                 */
                /// @custom:oz-upgrades-unsafe-allow delegatecall
                (bool success, ) = newImplementation.delegatecall(data);
                require(success, "upgrade call reverted");
            } else {
                _checkNonPayable();
            }
        } catch {
            revert("the implementation is not UUPS");
        }
    }

    function proxiableUUID() external view virtual returns (bytes32) {
        return _IMPLEMENTATION_SLOT;
    }

    function _checkNonPayable() private {
        if (msg.value > 0) {
            revert("non-payable upgrade call");
        }
    }

    function _setImplementation(address newImplementation) private {
        bytes32 slot = _IMPLEMENTATION_SLOT;
        // solhint-disable-next-line no-inline-assembly
        assembly {
            sstore(slot, newImplementation)
        }
    }
}
