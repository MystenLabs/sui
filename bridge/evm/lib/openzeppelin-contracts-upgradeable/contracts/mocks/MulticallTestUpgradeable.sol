// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC20MulticallMockUpgradeable} from "./token/ERC20MulticallMockUpgradeable.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract MulticallTestUpgradeable is Initializable {
    function __MulticallTest_init() internal onlyInitializing {
    }

    function __MulticallTest_init_unchained() internal onlyInitializing {
    }
    function checkReturnValues(
        ERC20MulticallMockUpgradeable multicallToken,
        address[] calldata recipients,
        uint256[] calldata amounts
    ) external {
        bytes[] memory calls = new bytes[](recipients.length);
        for (uint256 i = 0; i < recipients.length; i++) {
            calls[i] = abi.encodeCall(multicallToken.transfer, (recipients[i], amounts[i]));
        }

        bytes[] memory results = multicallToken.multicall(calls);
        for (uint256 i = 0; i < results.length; i++) {
            require(abi.decode(results[i], (bool)));
        }
    }
}
