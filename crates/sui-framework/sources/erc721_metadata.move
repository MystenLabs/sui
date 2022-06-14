// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::erc721_metadata {
    use std::ascii;
    use sui::url::{Self, Url};
    use sui::utf8;

    // TODO: add symbol()?
    /// A wrapper type for the ERC721 metadata standard https://eips.ethereum.org/EIPS/eip-721
    struct ERC721Metadata has store {
        /// The token id associated with the source contract on Ethereum
        token_id: TokenID,
        /// A descriptive name for a collection of NFTs in this contract.
        /// This corresponds to the `name()` method in the
        /// ERC721Metadata interface in EIP-721.
        name: utf8::String,
        /// A distinct Uniform Resource Identifier (URI) for a given asset.
        /// This corresponds to the `tokenURI()` method in the ERC721Metadata
        /// interface in EIP-721.
        token_uri: Url,
    }

    // TODO: replace u64 with u256 once the latter is supported
    // <https://github.com/MystenLabs/fastnft/issues/618>
    /// An ERC721 token ID
    struct TokenID has store, copy {
        id: u64,
    }

    /// Construct a new ERC721Metadata from the given inputs. Does not perform any validation
    /// on `token_uri` or `name`
    public fun new(token_id: TokenID, name: vector<u8>, token_uri: vector<u8>): ERC721Metadata {
        // Note: this will abort if `token_uri` is not valid ASCII
        let uri_str = ascii::string(token_uri);
        ERC721Metadata {
            token_id,
            name: utf8::string_unsafe(name),
            token_uri: url::new_unsafe(uri_str),
        }
    }

    public fun new_token_id(id: u64): TokenID {
        TokenID { id }
    }

    public fun token_id(self: &ERC721Metadata): &TokenID {
        &self.token_id
    }

    public fun token_uri(self: &ERC721Metadata): &Url {
        &self.token_uri
    }

    public fun name(self: &ERC721Metadata): &utf8::String {
        &self.name
    }
}
