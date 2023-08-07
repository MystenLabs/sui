---
title: Sui Validator nodes
---

Learn how to set up, configure, and manage a Sui Validator node, including staking, reference gas price, and tallying rules. 

## Requirements to run a validator on Sui

To run a Sui Validator, you must set up and configure a Sui Validator node. After you have a running node, you must have a minimum of 30 Million SUI in your staking pool to join the Validator set on the Sui network.

To learn how to set up and configure a Sui Validator node, see [Sui for Node Operators](https://github.com/MystenLabs/sui/blob/main/nre/sui_for_node_operators.md). The guide includes all of the information you need to configure your Validator node. It also provides guidance on the tasks you must perform after you join the validator set.

Specific steps you must take include:
 * Install and configure Sui
 * Configure Port and Protocol settings
 * Key management
 * Storage configuration
 * Software updates
 * On-chain commands
 * Update the Gas Price Survey
 * Reporting other validators

### Validator staking pool requirements

After you successfully join the validator set, you must maintain a staking pool with a minimum of 20 Million SUI staked. If, at any point, your staking pool falls below 15 Million SUI, your Validator node is removed from the committee at the next epoch boundary. Sui uses 24-hour epochs.

## Validator consensus and voting power

The total voting power on Sui is always 10,000, regardless of the amount staked. Therefore, the quorum threshold is 6,667. There is no limit to the amount of SUI users can stake with a validator. Each validator has consensus voting power proportional to SUI in its staking pool, with one exception: the voting power of an individual validator is capped at 1,000 (10% of the total). If a validator accumulates more than 10% of total stake, the validator’s voting power remains fixed at 10%, and the remaining voting power is spread across the rest of the validator set.

## Validator staking pool

Each Sui validator maintains its own staking pool to track the amount of stake and to compound staking rewards. Validator pools operate together with a time series of exchange rates that are computed at each epoch boundary. These exchange rates determine the amount of SUI tokens that each past SUI staker can withdraw in the future. Importantly, the exchange rates increase as more rewards are deposited into a staking pool and the longer an amount of SUI is deposited in a staking pool, the more rewards it will accrue.

When SUI is deposited to the staking pool in epoch E, those SUI are converted into liquidity tokens at the epoch E exchange rate. As the staking pool earns rewards, the exchange rate appreciates. At epoch E’, those liquidity tokens are worth more and translate into more SUI. 

The only difference between Sui staking pools and typical liquidity pools is that in Sui the liquidity tokens do not exist. Rather, the global exchange rate table is used to track the accounting. A nice feature of this design is that because all SUI tokens in the staking pool are treated the same, regardless of whether they were originally deposited as new stake or as stake rewards, all SUI tokens immediately count as stake and thus compound rewards immediately.

The staking pool is implemented in a system-level smart contract (staking_pool.move) and is part of the Sui Framework.

### User staking and withdrawals

When users stake SUI tokens, these SUI objects are wrapped into `StakedSUI` objects. The calculation to determine each user’s relative ownership of the staking pool is done directly with the timestamp of the `StakedSUI` object (which determines the moment at which the deposit took place) and the change in the exchange rates between the deposit epoch and the withdrawal epoch. Each staking pool’s data structure contains a time series with that pool’s exchange rates. These exchange rates can be used to determine the withdrawals of any of the pool’s stakers.

Stake withdrawals are processed immediately with the exchange rate prevailing at the previous epoch’s exchange rate. Withdrawals do not have to wait for the current epoch to close. Withdrawals include both the original stake the user deposited and all the stake rewards accumulated up to the previous epoch. Stakers do not earn the rewards accruing to their stake during the epoch at which they withdraw. Since there is no way to know how many stake rewards will be accumulated during the current epoch until the epoch closes, these cannot be included in the withdrawal. Hence, any user can withdraw their stake immediately and receive:

*SUI withdrawn at E’ = ( SUI deposited at E ) * ( Exchange Rate at E’-1 / Exchange Rate at E )*

## Validator pool exchange rate

The exchange rate for each validator pool is calculated at each epoch boundary as follows:

*Exchange Rate at E+1 = ( 1 + ( Third-Party Staker Rewards at E / Third-Party Stake at E ) ) *( Exchange Rate at E )*

The distinction between third-party owned vs validator-owned rewards and stake is relevant in that validators earn commissions on the staking pool’s tokens but third-party stakers do not. This accounting enables Sui to keep track of the rewards accrued by both validators and third-party token holders using a single global exchange rate table. 

### Find the exchange rate

Each epoch change emits a `0x3::validator_set::ValidatorEpochInfoEventV2` event per validator with the exchange rate information. You can use the Events API to query events.

## Staking rewards

Within a given validator staking pool, all stakers receive the same proportion of rewards through the pool’s exchange rate appreciation. In addition, since validators earn commissions over the stake they manage, validators receive additional `StakedSUI` objects at the end of each epoch in proportion to the amount of commissions their staking pool earns.

Staking rewards are funded by transaction gas fees collected during the current epoch and by stake subsidies released at the end of the epoch. 

*StakeRewards = StakeSubsidies + GasFees*

Stake subsidies are intended to subsidize the network during its early phases and are funded by a 10% allocation of SUI tokens. After this allocation depletes, the entirety of stake rewards will be made up of gas fees collected through regular network operations.

Stake rewards are made up of gas fees and stake subsidies. The total amount distributed throughout each epoch is determined as follows:

 * **Stake Subsidies:** The amount distributed in each epoch is determined prior to the beginning of the epoch according to a predefined schedule.
 * **Gas Fees:** Each epoch’s amount depends on the total gas fees collected throughout the epoch. Each Sui transaction pays gas fees depending on two variables, the amount of executed gas units and the gas price:

    *GasFee  = GasPrice * GasUnits*

The total amount of gas fees collected corresponds to the sum of gas fees across all transactions processed in the epoch. During regular market conditions, the vast majority of transactions should have a `GasPrice` equal to the `ReferenceGasPrice`.

### User staking and rewards

A stake deposit request goes into a pending state immediately in the staking pool as soon as it is made. Sui Wallet reflects any pending stake deposit requests for the user’s account. However, pending stake deposit requests do not take effect until the end of the epoch during which the request is made.

A withdrawal (un-stake) request is processed immediately as soon as it is received. The staker obtains the originally deposited SUI together with all accrued stake rewards up to the previous epoch boundary – in other words, they do not include stake rewards for the current epoch.

Users can’t withdraw a portion of their active stake. They must withdraw all staked SUI at the same time. Users can, however, stake using multiple `StakedSui` objects by splitting their SUI into multiple coins. They can then perform a partial withdrawal from a validator by un-staking only some of the `StakedSUI` objects. 

## Reference gas price

Sui is designed such that end-users can expect the gas price to be stable and predictable during regular network operations. This is achieved by having validators set the network’s reference gas price at the beginning of each epoch.

Operationally this is achieved through a Gas Price Survey that occurs as follows:
 1. During each epoch E, each validator submits what they think the optimal reference gas price should be for the next epoch E+1.
 2. At the epoch boundary, when Sui transitions from epoch E to epoch E+1, the network observes the gas price quotes across the validator set and sets the 2/3 percentile weighted by stake as the epoch’s reference gas price. Hence the reference gas price is constant throughout each epoch and is only updated when the epoch changes.

For example, assume that there are seven validators with equal stake, and the price quotes they submit are {15, 1, 4, 2, 8, 3, 23}. The protocol sets the reference gas price at 8.

In practice, the process for submitting a gas price quote for the Gas Price Survey is a straightforward one.  Each validator owns an object that contains their quote for the reference gas price. To change their response, they must update the value in that object.

For example, to set the price quote for the next epoch to 42, run:

```shell
sui client call --package 0x3 --module sui_system --function request_set_gas_price --args 0x5 \"42\" --gas-budget 1000
```

Importantly, the gas object’s value persists across epochs so that a validator who does not update and submit a new quote uses the same quote from the previous epoch. Hence, a validator seeking to optimize its own operations should update its quote every epoch in response to changes in network operations and market conditions.

## Validator slashing and tallying rule

Sui is designed to encourage and enforce community monitoring of the validator set. This is done through the Tallying Rule by which each validator monitors and scores every other validator in order to ensure that everyone is operating efficiently and in the network’s best interest. Validators that receive a low score can be penalized with slashed stake rewards.

The protocol only computes the global Tallying Rule score at the epoch boundary and so relies on validators monitoring actively and changing their individual scores whenever they detect changes in other validator behavior. In general, the Tallying Rule default option should always be a score of one for all validators and only be changed to zero upon determining bad operations. In practice, the Tallying Rule consists of a set of objects each validator owns that default to scores of one and thus a validator will generally be passive and only update the object corresponding to another validator’s score whenever needed.

For example, to report a validator whose Sui address is `0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446` as bad or non-performant, run:

```shell
sui client call --package 0x3 --module sui_system --function report_validator --args 0x5 0x44840a79dd5cf1f5efeff1379f5eece04c72db13512a2e31e8750f5176285446 --gas-budget 1000
```

The Tallying Rule should be implemented through a social equilibrium. The validator set should actively monitor itself and if one validator is clearly non-performant, then the other validators should score that validator with a 0 and slash its rewards. Community members can launch public dashboards tracking validator performance and that can be used as further signal into a validator’s operations. There is no limit on the number of validators that can receive a 0 tallying score in an epoch.
