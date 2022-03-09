// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Allow a trusted oracle to mint a copy of NFT from a different chain. There can
/// only be one copy for each unique pair of contract_address and token_id. We only
/// support a single chain(Ethereum) right now, but this can be extended to other
/// chains by adding a chain_id field. 
module Sui::CrossChainAirdrop {
    use Std::Vector;
    use Sui::ID::{VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// The oracle manages one `PerContractAirdropInfo` for each Ethereum contract
    struct CrossChainAirdropOracle has key {
        id: VersionedID,
        // TODO: replace this with SparseSet for O(1) on-chain uniqueness check
        managed_contracts: vector<PerContractAirdropInfo>
    }

    /// The token ID on the source contract
    struct SourceTokenID has store, copy {
        id: u64,
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
        claimed_source_token_ids: vector<SourceTokenID>
    }
    
    /// The Sui representation of the original NFT
    struct NFT has key {
        id: VersionedID,
        /// The address of the source contract, e.g, the Ethereum contract address
        source_contract_address: SourceContractAddress,
        // TODO: replace u64 with u256 once the latter is supported
        // <https://github.com/MystenLabs/fastnft/issues/618>
        /// The token id associated with the source contract e.g., the Ethereum token id
        source_token_id: SourceTokenID,
        /// A distinct Uniform Resource Identifier (URI) for a given asset.
        /// This corresponds to the `tokenURI()` method in the ERC721Metadata 
        /// interface in EIP-721.
        token_uri: vector<u8>,
        /// A descriptive name for a collection of NFTs in this contract. 
        /// This corresponds to the `name()` method in the
        /// ERC721Metadata interface in EIP-721.
        name: vector<u8>,
    }

    /// Address of the Oracle
    // TODO: Change this to something else before testnet launch
    const ORACLE_ADDRESS: address = @0xCEF1A51D2AA1226E54A1ACB85CFC58A051125A49;

    // Error codes

    /// Trying to claim a token that has already been claimed
    const ETOKEN_ID_CLAIMED: u64 = 0;

    /// Create the `Orcacle` capability and hand it off to the oracle
    fun init(ctx: &mut TxContext) {
        let oracle = oracle_address();
        Transfer::transfer(
            CrossChainAirdropOracle {
                id: TxContext::new_id(ctx),
                managed_contracts: Vector::empty(),
            },
            oracle
        )
    }

    /// Called by the oracle to mint the airdrop NFT and transfer to the recipient
    public fun claim(
        oracle: &mut CrossChainAirdropOracle,
        recipient: address,
        source_contract_address: vector<u8>,
        source_token_id: u64,
        name: vector<u8>,
        token_uri: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let contract = get_or_create_contract(oracle, &source_contract_address);
        // NOTE: this is where the globally uniqueness check happens
        assert!(!is_token_claimed(contract, source_token_id), ETOKEN_ID_CLAIMED);
        let token_id = SourceTokenID{ id: source_token_id };
        let coin = NFT {
            id: TxContext::new_id(ctx),
            source_contract_address: SourceContractAddress { address: source_contract_address },
            source_token_id: copy token_id,
            name,
            token_uri
        };
        Vector::push_back(&mut contract.claimed_source_token_ids, token_id);
        Transfer::transfer(coin, recipient);
    }

    fun get_or_create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let index = 0;
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        while (index < Vector::length(&oracle.managed_contracts)) {
            let info = Vector::borrow_mut(&mut oracle.managed_contracts, index);
            if (&info.source_contract_address.address == source_contract_address) {
                return info
            };
            index = index + 1;
        };
        
        create_contract(oracle, source_contract_address)
    }

    fun create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let info =  PerContractAirdropInfo {
            source_contract_address: SourceContractAddress { address: *source_contract_address },
            claimed_source_token_ids: Vector::empty()
        };
        Vector::push_back(&mut oracle.managed_contracts, info);
        let idx = Vector::length(&oracle.managed_contracts) - 1;
        Vector::borrow_mut(&mut oracle.managed_contracts, idx)
    }

    fun is_token_claimed(contract: &PerContractAirdropInfo, source_token_id: u64): bool {
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        let index = 0;
        while (index < Vector::length(&contract.claimed_source_token_ids)) {
            let claimed_id = Vector::borrow(&contract.claimed_source_token_ids, index);
            if (&claimed_id.id == &source_token_id) {
                return true
            };
            index = index + 1;
        };
        false
    }

    public fun oracle_address(): address {
        ORACLE_ADDRESS
    }

    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(ctx)
    }
}

