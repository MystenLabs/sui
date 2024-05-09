// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {OwnableUpgradeable} from "../../../access/OwnableUpgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

contract MyContractUpgradeable is Initializable, OwnableUpgradeable {
    function __MyContract_init(address initialOwner) internal onlyInitializing {
        __Ownable_init_unchained(initialOwner);
    }

    function __MyContract_init_unchained(address) internal onlyInitializing {}

    function normalThing() public {
        // anyone can call this normalThing()
    }

    function specialThing() public onlyOwner {
        // only the owner can call specialThing()!
    }
}
