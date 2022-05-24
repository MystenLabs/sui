---
title: Sui’s Delegated Proof-of-Stake System
---

The Sui platform relies on delegated proof-of-stake to determine the set of validators who process transactions. 

## SUI token delegation

Within each epoch, operations are processed by a fixed set of validators, each with a specific amount of stake delegated from SUI token holders. A validator's share of total stake is relevant in that it determines each validator’s share of voting power for processing transactions. Delegating SUI implies the SUI tokens are locked for the entire epoch. SUI token holders are free to unstake their SUI or to change their delegate validator when the epoch changes.

## Economic model

We now discuss how the different components of the Sui economy interact with each other in order to introduce Sui’s delegated proof-of-stake system. As a complementary reference, see the staking and tokenomics diagram in the [Sui Tokenomics](index.md) overview.

The Sui economic model works as follows:

1. At the beginning of each epoch: Three important things happen:
    1. SUI holders delegate (some) of their tokens to validators and a new [committee](../architecture/validators#committees ) is formed. 
    1. The reference gas prices are set as described in Sui’s [gas price mechanism](gas-pricing.md)
    1. The [storage fund’s](storage-fund.md) size is adjusted using the previous epoch’s net inflow.

    Following these actions, the protocol computes the total amount of stake as the sum of delegated stake plus the storage fund. Call the share of delegated stake $\alpha$.

1. During each epoch: Users submit transactions to the Sui platform and validators process them. For each transaction, users pay the associated computation and storage gas fees. In cases where users delete previous transaction data, users obtain a partial rebate of their storage fees. Validators observe the behavior of other validators and evaluate each other’s performance.
1. At the end of each epoch: The protocol distributes stake rewards to participants of the proof-of-stake mechanism. This occurs through two main steps:
    1. The total amount of stake rewards is calculated as the sum of computation fees accrued throughout the epoch plus the epoch’s stake reward subsidies. The latter component is optional in that it will disappear in the long run as the amount of SUI in circulation reaches its total supply.
    1. The total amount of stake rewards is distributed across various entities. Importantly, remember that the storage fund is taken into account in the calculation of the epoch’s total stake. However, the storage fund is not owned by any entities in the way that delegated SUI is. Instead, Sui’s economic model distributes the stake rewards accruing to the storage fund  – a share $(1-\alpha)$ of the total stake rewards – to validators in order to compensate them for their storage costs. Of these rewards, a share $\gamma$ is paid out to validators while the remaining $(1-\gamma)$ is used to reinvest in the fund’s capital. Finally, assume that validators charge a commission $\delta\\%$ on SUI token holders as a fee for delegation. The split of stake rewards across participants is given by: 

    	$$ DelegatorRewards \ \ = \ \ (  1 - \delta ) \ \times \  \alpha \ \times \ StakeRewards $$

    	$$ ValidatorRewards \ \ = \ \ ( \ \delta\alpha \ + \ \gamma (1 - \alpha) \ ) \ \times \ StakeRewards $$

    	$$ Reinvestment \ \ = \ \ ( 1 - \gamma ) \ \times \ ( 1 - \alpha ) \ \times \ StakeRewards $$

## Stake reward distribution

Sui’s gas pricing mechanism together with its delegated proof-of-stake mechanism jointly deliver an efficient economic model whereby validators are encouraged to operate smoothly with low but sustainable gas fees. A specific validator $v$ receives stake rewards equal to:

$$ ValidatorRewards(v) \ \ = \ \ RewardShare(v) \ \times \ ValidatorRewards $$

Where the $RewardShare(v)$ is determined in the [gas price mechanism](gas-pricing.md). Note that SUI token holders receive the same share of stake rewards as their delegate validator. Specifically, SUI token holders delegating at a validator $v$ obtain rewards equal to:

$$ DelegatorRewards(v) \ \ = \ \ RewardShare(v) \ \times \ DelegatorRewards $$

On net, this design encourages validators to operate with low gas price quotes – but not too low or else they receive slashed stake rewards. Consequently, Sui’s gas price mechanism and delegated proof-of-stake system encourages a healthy competition for fair prices where validators set low gas fees while operating with viable business models.

## Sui incentives

Sui’s economic model bestows Sui users with an important monitoring role. On the one hand, users want their transactions to be processed as quickly and efficiently as possible. User clients such as wallets encourage this by prioritizing communication with the most responsive validators. Such efficient operations are compensated with boosted rewards relative to less responsive validators. On the other hand, SUI token delegators receive the same boosted or penalized rewards as their delegate validator. An unresponsive validator is thus doubly exposed to Sui’s incentives: they lose directly through slashed rewards and indirectly through reduced delegated stake in future epochs as stakers move their tokens to more responsive validators.
