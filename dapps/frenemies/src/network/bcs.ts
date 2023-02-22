// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs as suiBcs } from "@mysten/sui.js";
import {
  ASSIGNMENT,
  DELEGATION,
  GENERIC_COIN,
  LEADERBOARD,
  OLD_SCORECARD,
  SCORECARD,
  SCORECARD_UPDATED,
  STAKED_SUI,
  SUI_SYSTEM,
  SYSTEM_EPOCH_INFO,
} from "./types";

export const bcs = suiBcs
  .registerStructType(ASSIGNMENT, {
    validator: "address",
    goal: "u8",
    epoch: "u64",
  })
  .registerStructType(SCORECARD, {
    id: "address",
    name: "string",
    assignment: ASSIGNMENT,
    score: "u16",
    participation: "u16",
    epoch: "u64",
  })
  .registerStructType(OLD_SCORECARD, {
    id: "address",
    name: "string",
    assignment: ASSIGNMENT,
    score: "u16",
    participation: "u16",
    epoch: "u64",
  })
  .registerStructType(SCORECARD_UPDATED, {
    scorecard: "address",
    assignment: ASSIGNMENT,
    totalScore: "u16",
    epochScore: "u16",
  })
  .registerStructType(LEADERBOARD, {
    id: "address",
    topScores: "vector<leaderboard::Score>",
    prevEpochStakes: "table::Table",
    epoch: "u64",
    startEpoch: "u64",
  })
  .registerStructType("leaderboard::Score", {
    name: "string",
    score: "u16",
    participation: "u16",
  })
  // This type only contains utility data;
  // Other fields (based on generics) are attached as dynamic fields.
  .registerStructType("table::Table", {
    id: "address",
    size: "u64",
  })
  // Sui System + Validators schema
  .registerStructType(GENERIC_COIN, {
    id: "address",
    value: "u64",
  })
  .registerStructType(STAKED_SUI, {
    id: "address",
    validatorAddress: "address",
    poolStartingEpoch: "u64",
    delegationRequestEpoch: "u64",
    staked: "u64",
    suiTokenLock: "Option<u64>",
  })
  .registerStructType(DELEGATION, {
    id: "address",
    stakedSuiId: "address",
    // Balance<DelegationToken> = u64
    poolTokens: "u64",
    principalSuiAmount: "u64",
  })
  .registerStructType(SUI_SYSTEM, {
    id: "address",
    epoch: "u64",
    validators: "validator_set::ValidatorSet",
  })
  .registerStructType(SYSTEM_EPOCH_INFO, {
    epoch: "u64",
    referenceGasPrice: "u64",
    totalStake: "u64",
    storageFundInflows: "u64",
    storageFundOutflows: "u64",
    storageFundBalance: "u64",
    stakeSubsidyAmount: "u64",
    totalGasFees: "u64",
    totalStakeRewards: "u64",
  })
  .registerStructType("validator_set::ValidatorSet", {
    /** Total amount of stake from all active validators (not including delegation), at the beginning of the epoch. */
    totalValidatorStake: "u64",
    /** Total amount of stake from delegation, at the beginning of the epoch. */
    totalDelegationStake: "u64",
    /** The current list of active validators. */
    activeValidators: "vector<validator::Validator>",
    /** List of new validator candidates added during the current epoch. They will be processed at the end of the epoch. */
    pendingValidators: "vector<validator::Validator>",
    /** Removal requests from the validators. Each element is an index pointing to `active_validators`. */
    pendingRemovals: "vector<u64>",
    /** The metadata of the validator set for the next epoch. This is kept up-to-dated. */
    nextEpochValidators: "vector<validator::ValidatorMetadata>",
    /**
     * Delegation switches requested during the current epoch, processed at epoch boundaries
     * so that all the rewards with be added to the new delegation.
     */
    // pendingDelegationSwitches: 'VecMap<ValidatorPair, table::Table>',
  })
  .registerStructType("validator_set::ValidatorPair", {
    from: "address",
    to: "address",
  })

  .registerStructType("validator::Validator", {
    /** Summary of the validator. */
    metadata: "validator::ValidatorMetadata",
    /** The voting power of this validator, which might be different from its stake amount. */
    votingPower: "u64",
    /** The current active stake amount. This will not change during an epoch. It can only be updated at the end of epoch. */
    stakeAmount: "u64",
    /** Pending stake deposit amount, processed at end of epoch. */
    pendingStake: "u64",
    /** Pending withdraw amount, processed at end of epoch. */
    pendingWithdraw: "u64",
    /** Gas price quote, updated only at end of epoch. */
    gasPrice: "u64",
    /** Staking pool for the stakes delegated to this validator. */
    delegationStakingPool: "staking_pool::StakingPool",
    /** Commission rate of the validator, in basis point. */
    commissionRate: "u64",
  })

  .registerStructType("validator::ValidatorMetadata", {
    /**
     * The Sui Address of the validator. This is the sender that created the Validator object
     * and also the address to send validator/coins to during withdraws.
     */
    suiAddress: "address",
    /**
     * The public key bytes corresponding to the private key that the validator
     * holds to sign transactions. For now, this is the same as AuthorityName.
     */
    pubkeyBytes: "vector<u8>",
    /**
     * The public key bytes corresponding to the private key that the validator
     * uses to establish TLS connections
     */
    networkPubkeyBytes: "vector<u8>",
    /** The public key bytes correstponding to the Narwhal Worker  */
    workerPubkeyBytes: "vector<u8>",
    /** This is a proof that the validator has ownership of the private key  */
    proofOfPossession: "vector<u8>",
    /**A unique human-readable name of this validator.  */
    name: "string",
    description: "string",
    imageUrl: "string",
    projectUrl: "string",
    /** The network address of the validator (could also contain extra info such as port, DNS and etc.).  */
    netAddress: "vector<u8>",
    /** The address of the narwhal primary  */
    consensusAddress: "vector<u8>",
    /** The address of the narwhal worker  */
    workerAddress: "vector<u8>",
    /** Total amount of validator stake that would be active in the next epoch.  */
    nextEpochStake: "u64",
    /** Total amount of delegated stake that would be active in the next epoch.  */
    nextEpochDelegation: "u64",
    /** This validator's gas price quote for the next epoch.  */
    nextEpochGasPrice: "u64",
    /** The commission rate of the validator starting the next epoch, in basis point.  */
    nextEpochCommissionRate: "u64",
  })

  .registerEnumType("Option<T>", {
    none: null,
    some: "T",
  })

  .registerStructType("linked_table::LinkedTable<T>", {
    id: "address",
    size: "u64",
    head: "Option<T>",
    tail: "Option<T>",
  })

  .registerStructType("staking_pool::StakingPool", {
    /// The sui address of the validator associated with this pool.
    validatorAddress: "address",
    /// The epoch at which this pool started operating. Should be the epoch at which the validator became active.
    startingEpoch: "u64",
    /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
    /// in the `Delegation` object, updated at epoch boundaries.
    suiBalance: "u64",
    /// The epoch delegation rewards will be added here at the end of each epoch.
    rewardsPool: "u64",
    /// The number of delegation pool tokens we have issued so far. This number should equal the sum of
    /// pool token balance in all the `Delegation` objects delegated to this pool. Updated at epoch boundaries.
    delegationTokenSupply: "u64",
    /// Delegations requested during the current epoch. We will activate these delegation at the end of current epoch
    /// and distribute staking pool tokens at the end-of-epoch exchange rate after the rewards for the current epoch
    /// have been deposited.
    pendingDelegations: "linked_table::LinkedTable<address>", // second parameter is phantom
    /// Delegation withdraws requested during the current epoch. Similar to new delegation, the withdraws are processed
    /// at epoch boundaries. Rewards are withdrawn and distributed after the rewards for the current epoch have come in.
    pendingWithdraws: "table::Table",
  });
