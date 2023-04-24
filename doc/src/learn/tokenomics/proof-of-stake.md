---
title: Sui’s Delegated Proof-of-Stake System
---

The Sui platform relies on Delegated Proof-of-Stake to determine the set of validators who process transactions. 

## SUI token staking

Within each epoch, operations are processed by a fixed set of validators, each with a specific amount of stake from SUI token holders. A validator's share of total stake is relevant in that it determines each validator’s share of voting power for processing transactions. Staking SUI implies the SUI tokens are locked for the entire epoch. SUI token holders are free to withdraw their SUI or to change their selected validator when the epoch changes.

## Economic model

This section covers how the different components of the Sui economy interact with each other in order to introduce Sui’s Delegated Proof-of-Stake system. As a complementary reference, see the Staking and Tokenomics diagram in the [Sui Tokenomics](../tokenomics/index.md) overview.

The Sui economic model works as follows:

At the beginning of each epoch, three important things happen:
 * SUI holders stake (some) of their tokens to validators and a new [committee](../architecture/validators#committees) is formed. 
 * The reference gas prices are set as described in Sui’s [gas price mechanism](gas-pricing.md).
 * The [storage fund’s](storage-fund.md) size is adjusted using the previous epoch’s net inflow.
  
Following these actions, the protocol computes the total amount of stake as the sum of staked SUI plus the storage fund. Call the share of user stake $\alpha$.

During each epoch: Users submit transactions to the Sui platform and validators process them. For each transaction, users pay the associated computation and storage gas fees. In cases where users delete previous transaction data, users obtain a partial rebate of their storage fees. Validators observe the behavior of other validators and evaluate each other’s performance.

At the end of each epoch: The protocol distributes stake rewards to participants of the proof-of-stake mechanism. This occurs through two main steps:
 * The total amount of stake rewards is calculated as the sum of computation fees accrued throughout the epoch plus the epoch’s stake reward subsidies. The latter component is temporary in that it will only exist in the network's first years and disappear in the long run as the amount of SUI in circulation reaches its total supply.
 * The total amount of stake rewards is distributed across various entities. Importantly, remember that the storage fund is taken into account in the calculation of the epoch’s total stake. However, the storage fund is not owned by any entities in the way that staked SUI is. Instead, Sui’s economic model distributes the stake rewards accruing to the storage fund  – a share $(1-\alpha)$ of the total stake rewards – to validators in order to compensate them for their storage costs. Of these rewards, a share $\gamma$ is paid out to validators while the remaining $(1-\gamma)$ is used to reinvest in the fund’s capital. Finally, let $\beta_v$ represent the share of stake managed by a validator $v$ that is owned by itself while $(1-\beta_v)$ represents the share owned by third-party stakers. Validators keep the full rewards accruing to their own stake but keep only a commission $\delta_v\\%$ on SUI tokens staked by users as a fee for managing that stake. The split of stake rewards for the user staking pool staking at validator $v$ and for the validator itself equal: 

$$ UserStakeRewards_v \ = \Big[ \alpha(1-\delta_v)(1-\beta_v)\Big]\mu_v\sigma_v \times StakeRewards $$

$$ ValidatorRewards_v \ = \ \Bigg[\alpha\Big(\beta_v+\delta_v(1-\beta_v)\Big)\mu_v\sigma_v+(1-\alpha)\frac{\gamma}{N}\Bigg] \times StakeRewards $$

The $\mu_v$ variable captures the output of the [tallying rule](gas-pricing.md#tallying-rule) computed as part of the [gas price mechanism](gas-pricing.md) and corresponds to $\mu_v\geq1$ for performant validators and $\mu_v<1$ for non-performant validators. This variable ensures that validators have "skin in the game" and are incentivized to operate Sui efficiently. The $\sigma_v$ parameter captures each validator's share of total stake. 

Consequently, validators with more stake earn more stake rewards and the joint $\mu_v\sigma_v$ term incentivizes validators to increase their share of stake while also operating the network performantly. In the long-run, this incentive encourages users to shift the stake distribution towards the network's most efficient validators, delivering a cost-efficient and decentralized network.

Finally, note that the storage fund rewards accrue to all $N$ validators equally (since all validators face a similar burden of holding data in storage). The small amount of stake rewards that is not distributed out to either users or validators, namely the share $(1-\alpha)(1-\gamma)$ of stake rewards, gets reinvested in the storage fund.

On net, this design encourages validators to operate with low gas price quotes – but not too low or else they receive slashed stake rewards. Consequently, Sui’s gas price mechanism and Delegated Proof-of-Stake system encourages a healthy competition for fair prices where validators set low gas fees while operating with viable business models.

## Sui incentives

Sui’s economic model bestows Sui users with an important monitoring role. On the one hand, users want their transactions to be processed as quickly and efficiently as possible. User clients such as wallets encourage this by prioritizing communication with the most responsive validators. Such efficient operations are compensated with boosted rewards relative to less responsive validators. On the other hand, SUI token stakers receive the same boosted or penalized rewards as their selected validator. An unresponsive validator is thus doubly exposed to Sui’s incentives: they lose directly through slashed rewards and indirectly through reduced user stake in future epochs as stakers move their tokens to more responsive validators.
