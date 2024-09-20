// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

import {ERC721URIStorageUpgradeable} from "../../token/ERC721/extensions/ERC721URIStorageUpgradeable.sol";
import {Initializable} from "../../proxy/utils/Initializable.sol";

abstract contract ERC721URIStorageMockUpgradeable is Initializable, ERC721URIStorageUpgradeable {
    string private _baseTokenURI;

    function __ERC721URIStorageMock_init() internal onlyInitializing {
    }

    function __ERC721URIStorageMock_init_unchained() internal onlyInitializing {
    }
    function _baseURI() internal view virtual override returns (string memory) {
        return _baseTokenURI;
    }

    function setBaseURI(string calldata newBaseTokenURI) public {
        _baseTokenURI = newBaseTokenURI;
    }
}
