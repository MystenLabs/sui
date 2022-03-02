// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Allow a trusted oracle to mint a copy of NFT from a different chain. There can
/// only be one copy for each unique pair of contract_address and token_id. We only
/// support a signle chain(Ethereum) right now, but this can be extended to other
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

    /// Contains the Airdrop info for one contract address on Ethereum
    struct PerContractAirdropInfo has store {
        /// A single contract address on Ethereum
        source_contract_address: vector<u8>,

        /// The Ethereum token ids whose Airdrop has been claimed. These
        /// are stored to prevent the same NFT from being claimed twice
        // TODO: replace u128 with u256 once the latter is supported
        // <https://github.com/MystenLabs/fastnft/pull/616>
        // TODO: replace this with SparseSet for O(1) on-chain uniqueness check
        claimed_source_token_ids: vector<u128>
    }
    
    /// The Sui representation of the original NFT
    struct NFT has key {
        id: VersionedID,
        /// The Ethereum contract address
        source_contract_address: vector<u8>,
        /// The Ethereum token id associated with the source contract address
        source_token_id: u128,
        /// A distinct Uniform Resource Identifier (URI) for a given asset.
        /// This corresponds to the `tokenURI()` method in the ERC721Metadata 
        /// interface in EIP-721.
        token_uri: vector<u8>,

        // TODO: the `name` and `symbol` field below are contract level information,
        // and should be put in `PerContractAirdropInfo`. However, doing so will
        // make the rendering of an object hard as the front-end need to 
        // fetch the info from another object. 

        /// A descriptive name for a collection of NFTs in this contract. 
        /// This corresponds to the `name()` method in the
        /// ERC721Metadata interface in EIP-721.
        name: vector<u8>,
        /// An abbreviated name for NFTs in the contract. This corresponds to 
        /// the `symbol()` method in the ERC721Metadata interface in EIP-721.
        symbol: vector<u8>
    }

    /// Address of the Oracle
    // TODO: Change this to something else before testnet launch
    const ORACLE_ADDRESS: address = @0x8c028a9e8e11ef91187153190d30c833b70338c9;

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
        ctx: &mut TxContext,
        oracle: &mut CrossChainAirdropOracle,
        recipient: address,
        source_contract_address: vector<u8>,
        source_token_id: u128,
        name: vector<u8>,
        symbol: vector<u8>,
        token_uri: vector<u8>
    ) {
        let contract = get_or_create_contract(oracle, &source_contract_address);
        // NOTE: this is where the globally uniqueness check happens
        assert!(!token_claimed(contract, source_token_id), ETOKEN_ID_CLAIMED);
        let coin = NFT {
            id: TxContext::new_id(ctx),
            source_contract_address,
            source_token_id,
            name,
            symbol,
            token_uri
        };
        Vector::push_back(&mut contract.claimed_source_token_ids, source_token_id);
        Transfer::transfer(coin, recipient);
    }

    fun get_or_create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let index = 0;
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        while (index < Vector::length(&oracle.managed_contracts)) {
            let info = Vector::borrow_mut(&mut oracle.managed_contracts, index);
            if (&info.source_contract_address == source_contract_address) {
                return info
            };
            index = index + 1;
        };
        
        create_contract(oracle, source_contract_address)
    }

    fun create_contract(oracle: &mut CrossChainAirdropOracle, source_contract_address: &vector<u8>): &mut PerContractAirdropInfo {
        let info =  PerContractAirdropInfo {
            source_contract_address: *source_contract_address,
            claimed_source_token_ids: vector[]
        };
        Vector::push_back(&mut oracle.managed_contracts, info);
        let idx = Vector::length(&oracle.managed_contracts) - 1;
        Vector::borrow_mut(&mut oracle.managed_contracts, idx)
    }

    fun token_claimed(contract: &PerContractAirdropInfo, source_token_id: u128): bool {
        // TODO: replace this with SparseSet so that the on-chain uniqueness check can be O(1)
        let index = 0;
        while (index < Vector::length(&contract.claimed_source_token_ids)) {
            let claimed_id = Vector::borrow(&contract.claimed_source_token_ids, index);
            if (claimed_id == &source_token_id) {
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

