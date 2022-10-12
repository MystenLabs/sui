// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Allow a trusted oracle to mint a copy of NFT from a different chain. There can
/// only be one copy for each unique pair of contract_address and token_id. We only
/// support a single chain(Ethereum) right now, but this can be extended to other
/// chains by adding a chain_id field.
module nfts::cross_chain_airdrop {
    use std::vector;
    use sui::erc721_metadata::{Self, ERC721Metadata, TokenID};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// The oracle manages one `PerContractAirdropInfo` for each Ethereum contract
    struct CrossChainAirdropOracle has key {
        id: UID,
        // TODO: replace this with SparseSet for O(1) on-chain uniqueness check
        managed_contracts: vector<PerContractAirdropInfo>,
    }

    /// The address of the source contract
    struct SourceContractAddress has store, copy {
        address: vector<u8>,
    }

    /// Contains the Airdrop info for one contract address on Ethereum
    struct PerContractAirdropInfo has store {
        /// A single contract address on Ethereum
        source_contract_address: SourceContractAddress,

        /// The Ethereum token ids whose Airdrop has been claimed. These
        /// are stored to prevent the same NFT from being claimed twice
        // TODO: replace u64 with u256 once the latter is supported
        // <https://github.com/MystenLabs/fastnft/issues/618>
        // TODO: replace this with SparseSet for O(1) on-chain uniqueness check
        claimed_source_token_ids: vector<TokenID>
    }

    /// The Sui representation of the original ERC721 NFT on Eth
    struct ERC721 has key, store {
        id: UID,
        /// The address of the source contract, e.g, the Ethereum contract address
        source_contract_address: SourceContractAddress,
        /// The metadata associated with this NFT
        metadata: ERC721Metadata,
    }

    // Error codes

    /// Trying to claim a token that has already been claimed
    const ETokenIDClaimed: u64 = 0;

    /// Create the `Orcacle` capability and hand it off to the oracle
    /// TODO: To make this usable, the oracle should be sent to a
    /// hardcoded address that the contract creator has private key.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            CrossChainAirdropOracle {
                id: object::new(ctx),
                managed_contracts: vector::empty(),
            },
            tx_context::sender(ctx)
        )
    }

    /// Called by the oracle to mint the airdrop NFT and transfer to the recipient
    public entry fun claim(
        oracle: &mut CrossChainAirdropOracle,
        recipient: address,
        source_contract_address: vector<u8>,
        source_token_id: u64,
        name: vector<u8>,
        token_uri: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let contract = get_or_create_contract(oracle, &source_contract_address);
        let token_id = erc721_metadata::new_token_id(source_token_id);
        // NOTE: this is where the globally uniqueness check happens
        assert!(!is_token_claimed(contract, &token_id), ETokenIDClaimed);
        let nft = ERC721 {
            id: object::new(ctx),
            source_contract_address: SourceContractAddress { address: source_contract_address },
            metadata: erc721_metadata::new(token_id, name, token_uri),
        };
        vector::push_back(&mut contract.claimed_source_token_ids, token_id);
        transfer::transfer(nft, recipient)
    }

    fun get_or_create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let index = 0;
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        while (index < vector::length(&oracle.managed_contracts)) {
            let id = vector::borrow_mut(&mut oracle.managed_contracts, index);
            if (&id.source_contract_address.address == source_contract_address) {
                return id
            };
            index = index + 1;
        };

        create_contract(oracle, source_contract_address)
    }

    fun create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let id =  PerContractAirdropInfo {
            source_contract_address: SourceContractAddress { address: *source_contract_address },
            claimed_source_token_ids: vector::empty()
        };
        vector::push_back(&mut oracle.managed_contracts, id);
        let idx = vector::length(&oracle.managed_contracts) - 1;
        vector::borrow_mut(&mut oracle.managed_contracts, idx)
    }

    fun is_token_claimed(contract: &PerContractAirdropInfo, source_token_id: &TokenID): bool {
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        let index = 0;
        while (index < vector::length(&contract.claimed_source_token_ids)) {
            let claimed_id = vector::borrow(&contract.claimed_source_token_ids, index);
            if (claimed_id == source_token_id) {
                return true
            };
            index = index + 1;
        };
        false
    }

    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(ctx)
    }
}
