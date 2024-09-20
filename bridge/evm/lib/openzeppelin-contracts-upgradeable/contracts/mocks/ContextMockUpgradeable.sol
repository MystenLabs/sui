// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ContextUpgradeable} from "../utils/ContextUpgradeable.sol";
import {Initializable} from "../proxy/utils/Initializable.sol";

contract ContextMockUpgradeable is Initializable, ContextUpgradeable {
    event Sender(address sender);

    function __ContextMock_init() internal onlyInitializing {
    }

    function __ContextMock_init_unchained() internal onlyInitializing {
    }
    function msgSender() public {
        emit Sender(_msgSender());
    }

    event Data(bytes data, uint256 integerValue, string stringValue);

    function msgData(uint256 integerValue, string memory stringValue) public {
        emit Data(_msgData(), integerValue, stringValue);
    }

    event DataShort(bytes data);

    function msgDataShort() public {
        emit DataShort(_msgData());
    }
}

contract ContextMockCallerUpgradeable is Initializable {
    function __ContextMockCaller_init() internal onlyInitializing {
    }

    function __ContextMockCaller_init_unchained() internal onlyInitializing {
    }
    function callSender(ContextMockUpgradeable context) public {
        context.msgSender();
    }

    function callData(ContextMockUpgradeable context, uint256 integerValue, string memory stringValue) public {
        context.msgData(integerValue, stringValue);
    }
}
