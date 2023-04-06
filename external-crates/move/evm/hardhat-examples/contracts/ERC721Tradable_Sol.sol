// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";

/**
 * @title ERC721Mock
 * This mock just provides a public safeMint, mint, and burn functions for testing purposes
 */
contract ERC721Tradable_Sol is ERC721 {
    string private baseURI_;
    constructor(string memory name, string memory symbol, string memory newBaseURI) ERC721(name, symbol) {
        baseURI_ = newBaseURI;

        _mint(msg.sender, 1);
        _mint(msg.sender, 2);
        _mint(msg.sender, 3);
    }

    function _baseURI() internal override view returns (string memory) {
        return baseURI_;
    }

    function baseURI() public view returns (string memory) {
        return _baseURI();
    }

    function exists(uint256 tokenId) public view returns (bool) {
        return _exists(tokenId);
    }

    function mint(address to, uint256 tokenId) public {
        _mint(to, tokenId);
    }

    function safeMint(address to, uint256 tokenId) public {
        _safeMint(to, tokenId);
    }

    function safeMint(
        address to,
        uint256 tokenId,
        bytes memory _data
    ) public {
        _safeMint(to, tokenId, _data);
    }

    function burn(uint256 tokenId) public {
        _burn(tokenId);
    }
}
