// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ERC721Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC721/ERC721Upgradeable.sol";

// import "@openzeppelin/contracts/proxy/ERC1967/ERC1967Upgrade.sol";

contract MyProxy is ERC721Upgradeable {
    // constructor(
    //     address _logic,
    //     bytes memory _data
    // ) ERC1967Proxy(_logic, _data) {}

    function initialize() public initializer {
        __ERC721_init("MyCollectible", "MCO");
    }

    // Function to expose the address of the current implementation
    function getImplementationAddress() public view returns (address) {
        // return ERC721Upgradeable._getImplementation();
    }

    // Function to upgrade to a new implementation
    function upgradeTo(address newImplementation) public {
        // ERC721Upgradeable._upgradeTo(newImplementation);
    }
}
