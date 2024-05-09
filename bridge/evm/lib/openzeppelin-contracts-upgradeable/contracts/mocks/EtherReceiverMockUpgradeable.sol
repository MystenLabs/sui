// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;
import {Initializable} from "../proxy/utils/Initializable.sol";

contract EtherReceiverMockUpgradeable is Initializable {
    bool private _acceptEther;

    function __EtherReceiverMock_init() internal onlyInitializing {
    }

    function __EtherReceiverMock_init_unchained() internal onlyInitializing {
    }
    function setAcceptEther(bool acceptEther) public {
        _acceptEther = acceptEther;
    }

    receive() external payable {
        if (!_acceptEther) {
            revert();
        }
    }
}
