// SPDX-License-Identifier: MIT
// OpenZeppelin Contracts (last updated v5.0.0) (token/ERC1155/extensions/ERC1155URIStorage.sol)

pragma solidity ^0.8.20;

import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";
import {ERC1155Upgradeable} from "../ERC1155Upgradeable.sol";
import {Initializable} from "../../../proxy/utils/Initializable.sol";

/**
 * @dev ERC1155 token with storage based token URI management.
 * Inspired by the ERC721URIStorage extension
 */
abstract contract ERC1155URIStorageUpgradeable is Initializable, ERC1155Upgradeable {
    using Strings for uint256;

    /// @custom:storage-location erc7201:openzeppelin.storage.ERC1155URIStorage
    struct ERC1155URIStorageStorage {
        // Optional base URI
        string _baseURI;

        // Optional mapping for token URIs
        mapping(uint256 tokenId => string) _tokenURIs;
    }

    // keccak256(abi.encode(uint256(keccak256("openzeppelin.storage.ERC1155URIStorage")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ERC1155URIStorageStorageLocation = 0x89fc852226e759c7c636cf34d732f0198fc56a54876b2374a52beb7b0c558600;

    function _getERC1155URIStorageStorage() private pure returns (ERC1155URIStorageStorage storage $) {
        assembly {
            $.slot := ERC1155URIStorageStorageLocation
        }
    }

    function __ERC1155URIStorage_init() internal onlyInitializing {
        __ERC1155URIStorage_init_unchained();
    }

    function __ERC1155URIStorage_init_unchained() internal onlyInitializing {
        ERC1155URIStorageStorage storage $ = _getERC1155URIStorageStorage();
        $._baseURI = "";
    }
    /**
     * @dev See {IERC1155MetadataURI-uri}.
     *
     * This implementation returns the concatenation of the `_baseURI`
     * and the token-specific uri if the latter is set
     *
     * This enables the following behaviors:
     *
     * - if `_tokenURIs[tokenId]` is set, then the result is the concatenation
     *   of `_baseURI` and `_tokenURIs[tokenId]` (keep in mind that `_baseURI`
     *   is empty per default);
     *
     * - if `_tokenURIs[tokenId]` is NOT set then we fallback to `super.uri()`
     *   which in most cases will contain `ERC1155._uri`;
     *
     * - if `_tokenURIs[tokenId]` is NOT set, and if the parents do not have a
     *   uri value set, then the result is empty.
     */
    function uri(uint256 tokenId) public view virtual override returns (string memory) {
        ERC1155URIStorageStorage storage $ = _getERC1155URIStorageStorage();
        string memory tokenURI = $._tokenURIs[tokenId];

        // If token URI is set, concatenate base URI and tokenURI (via string.concat).
        return bytes(tokenURI).length > 0 ? string.concat($._baseURI, tokenURI) : super.uri(tokenId);
    }

    /**
     * @dev Sets `tokenURI` as the tokenURI of `tokenId`.
     */
    function _setURI(uint256 tokenId, string memory tokenURI) internal virtual {
        ERC1155URIStorageStorage storage $ = _getERC1155URIStorageStorage();
        $._tokenURIs[tokenId] = tokenURI;
        emit URI(uri(tokenId), tokenId);
    }

    /**
     * @dev Sets `baseURI` as the `_baseURI` for all tokens
     */
    function _setBaseURI(string memory baseURI) internal virtual {
        ERC1155URIStorageStorage storage $ = _getERC1155URIStorageStorage();
        $._baseURI = baseURI;
    }
}
