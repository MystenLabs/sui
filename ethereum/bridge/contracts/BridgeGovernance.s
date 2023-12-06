// contracts/Bridge.sol
// SPDX-License-Identifier: Apache 2

pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/proxy/ERC1967/ERC1967Upgrade.sol";

// import "../libraries/external/BytesLib.sol";

import "./BridgeGetters.sol";
// import "./BridgeSetters.sol";
// import "./BridgeStructs.sol";

// import "./token/Token.sol";
// import "./token/TokenImplementation.sol";

// import "../interfaces/IWormhole.sol";

// contract BridgeGovernance is BridgeGetters, BridgeSetters, ERC1967Upgrade {
contract BridgeGovernance is BridgeGetters, ERC1967Upgrade {
    // Execute a UpgradeContract governance message
    function upgrade(bytes memory encodedVM) public {
        require(!isFork(), "invalid fork");

        (
            IWormhole.VM memory vm,
            bool valid,
            string memory reason
        ) = verifyGovernanceVM(encodedVM);
        require(valid, reason);

        setGovernanceActionConsumed(vm.hash);

        BridgeStructs.UpgradeContract memory implementation = parseUpgrade(
            vm.payload
        );

        require(implementation.chainId == chainId(), "wrong chain id");

        upgradeImplementation(
            address(uint160(uint256(implementation.newContract)))
        );
    }

    event ContractUpgraded(
        address indexed oldContract,
        address indexed newContract
    );

    function upgradeImplementation(address newImplementation) internal {
        address currentImplementation = _getImplementation();

        _upgradeTo(newImplementation);

        // Call initialize function of the new implementation
        (bool success, bytes memory reason) = newImplementation.delegatecall(
            abi.encodeWithSignature("initialize()")
        );

        require(success, string(reason));

        emit ContractUpgraded(currentImplementation, newImplementation);
    }

}
