---
title: Learning Sui Tokenomics
---

Sui’s tokenomics are designed at the frontier of economic blockchain research, aiming to deliver an economic ecosystem and financial plumbing at par with Sui’s leading engineering design. 

This page includes a high-level overview of Sui’s economic model. For further details, refer to the tokenomics white paper: “The Sui Smart Contracts Platform: Economics and Incentives.”


## The Sui economy

The Sui economy is characterized by three main sets of participants:

* **Users** submit transactions to the Sui platform in order to create, mutate, and transfer digital assets or interact with more sophisticated applications enabled by smart contracts, interoperability, and composability.
* **SUI token holders** bear the option of delegating their tokens to validators and participating in the proof-of-stake mechanism. SUI owners also hold the rights to participate in Sui’s governance.
* **Validators** manage transaction processing and execution on the Sui platform.

The Sui economy has five core components:

* The SUI token is the Sui platform’s native asset. 
* Gas fees are charged on all network operations and used to reward participants of the proof-of-stake mechanism and prevent spam and denial-of-service attacks.
* Sui’s storage fund is used to shift stake rewards across time and compensate future validators for storage costs of previously stored on-chain data.
* The proof-of-stake mechanism is used to select, incentivize, and reward honest behavior by the Sui platform’s operators – i.e. validators and the SUI delegators.
* On-chain voting is used for governance and protocol upgrades.

Throughout, we use the visual representation in the following figure to aid the discussion. 

![Sui tokenomics flow](../../static/sui-tokenomics-flow.png "See staking and tokenomics in Sui")
*See staking and tokenomics in Sui*

# The SUI token

Sui’s native asset is called SUI and we generally use the capitalized version of SUI to distinguish the token from the Sui platform.

The total supply of SUI is capped at 10,000,000,000 (i.e. ten billion tokens). A share of SUI’s total supply will be liquid at mainnet launch, with the remaining tokens vesting over the coming years or distributed as future stake reward subsidies. Each SUI token is divisible up to a large number of decimal places.

The SUI token serves four purposes on the Sui platform:

* SUI can be staked within an epoch in order to participate in the proof-of-stake mechanism. 
* SUI is the asset denomination needed for paying the gas fees required to execute and store transactions or other operations on the Sui platform. 
* SUI can be used as a versatile and liquid asset for various applications including the standard features of money – a unit of account, a medium of exchange, or a store of value – and more complex functionality enabled by smart contracts, interoperability, and composability across the Sui ecosystem. 
* SUI token plays an important role in governance by acting as a right to participate in on-chain voting on issues such as protocol upgrades.

Since the SUI token is available in finite supply, SUI may face deflationary pressure in the long run if Sui unlocks more use cases and more users migrate to the platform. In addition, the storage fund’s presence creates an additional deflationary force in that higher on-chain data requirements translate into a larger storage fund, thus reducing the amount of SUI in circulation.

# Sui’s Gas Pricing Mechanism

Sui’s gas pricing mechanism achieves the triple outcomes of delivering users with low, predictable transaction fees, of incentivizing validators to optimize their transaction processing operations, and of preventing denial of service attacks. This delivers good user experience to Sui users, who can focus on using the Sui network without worrying about having to forecast the current market price of gas fees. Moreover, by rewarding good validator behavior, the gas mechanism aligns incentives between SUI token holders, the network’s operators (i.e. the validators), and its users.

A uniquekey feature of Sui’s gas price mechanism is that users pay separate fees for transaction execution and for storing the data associated with each transaction. The gas fees associated with an arbitrary transaction $\tau$ equal:

$GasFees[\tau] \ \ = \ \ ComputationUnits[\tau] \ \times \ ComputationPrice[\tau] \ \ + \ \ StorageUnits[\tau] \ \times \ StoragePrice$

The gas functions $ComputationUnits[\tau]$ and $StorageUnits[\tau]$ measure the amount of computation and storage resources, respectively, required to process and store the data associated with t. The gas prices $ConsumptionPrice[\tau]$ and $StoragePrice$ translate the cost of computation and storage, respectively, into SUI units. The coupling between gas units and gas prices is useful since SUI’s market price will fluctuate over time depending on demand and supply fluctuations.

## Computation gas prices

The computation gas price $ComputationPrice[\tau]$ captures the cost of one unit of computation in SUI units. This price is set at the transaction level and submitted by the user in two parts:

$$ComputationPrice[\tau] \ \ = \ \ ReferencePrice \ \ + \ \ Tip[\tau],\quad\text{ with }\quad Tip[\tau] \ > \ -ReferencePrice$$

The $ReferencePrice$ is fixed at the network level for the duration of the epoch, while the tip is at the user’s discretion. Since the tip can be negative, in practice the user can submit any gas price – as long as the overall price is positive. 

Sui’s gas price mechanism is intended to make the $ReferencePrice$ a credible anchor for users to use when submitting transactions to the network, thereby providing reasonable confidence that transactions submitted with gas prices at or close to the reference price will be executed in a timely manner. This is achieved through three core steps:

* _Gas Price Survey_ - A validatory-wide survey is conducted at the beginning of each epoch, and every validator submits their reservation price. That is, each validator states the minimum gas price at which they are willing to process transactions. The protocol orders these quotes and chooses the 2/3's percentile by stake as the reference price. The gas price survey’s goal is to set a reference price under which a quorum of validators are willing to promptly process transactions.
* _Tallying Rule_ - Throughout the epoch, validators obtain signals over the operations of other validators. Each validator uses these signals to build a (subjective) evaluation over the performance of every other validator. Specifically, each validator constructs a multiplier for the stake rewards of every other validator such that validators who behave well receive boosted rewards, and validators who do not receive reduced rewards. Good behavior is proxied by the share of transactions above a validator’s self-declared reservation price that the validator processed in a timely manner. The tallying rule’s goal is to create a community-enforced mechanism for encouraging validators to honor the quotes submitted during the gas survey.
* _Incentivized Stake Reward Distribution Rule_ - At the end of the epoch, the distribution of stake rewards across validators is adjusted using information from the gas price survey and tallying rule. Specifically, a global multiplier is built for every validator using the median value – weighted by stake – out of the set of individual multipliers constructed during the tallying rule. The incentivized stake reward distribution then sets the share of stake rewards distributed to each validator v as:

    $$ RewardShare(v) \ \ = \ \ Constant \ \times \ (\ 1 \ + \ GasSurveyBoost \) \ \times \ Multiplier(v) \ \times \ StakeShare(v) $$

    The Constant term is used as a normalization such that the sum of RewardShare(v) across the validator set sums up to one. If the validator submitted a price quote under the reference gas price, then $GasSurveyBoost \ > \ 0$. If not, $GasSurveyBoost \ < \ 0$. The purpose of this booster is to encourage validators to submit low reservation prices during the gas price survey. Finally, $Multiplier(v)$ is the global multiplier built from the subjective evaluations in the tallying rule. Note that in a symmetric equilibrium where all validators submit the same quote to the gas price survey and where all validators behave well as measured by the tallying rule, then $ RewardShare(v) \ = \ StakeShare(v)$ and each validator receives stake rewards in proportion to their share of overall stake.

In sum, the gas price mechanism has two main forces: the tallying rule incentivizes validators to honor the quotes submitted during the gas survey, while the distribution rule incentivizes validators to submit low reservations prices. The interaction of these two forces delivers a mechanism encouraging validators to set a low network-level reference gas price – but not too low since they face penalties if they cannot honor their quotes. In other words, the gas price mechanism encourages a healthy competition for fair prices.


## Storage gas prices

The storage gas price $StoragePrice$ captures the costs of covering one unit of storage in perpetuity, in SUI units. This price is set through governance proposals and is updated infrequently. The goal is to ensure Sui users pay for their use of on-chain data storage by depositing these fees into the storage fund and then redistributing these fees to future validators. In contrast to the computation gas price, storage prices are fixed and common for all transactions both within an epoch and across epochs until the storage price is updated.

The $StoragePrice$ is set exogenously through the governance proposal with the goal of targeting the off-chain dollar cost of data storage. In the long run, as the costs of storage fall due to technological improvements and the dollar price of the SUI token evolves, governance proposals will update the price in order to reflect the new dollar target price.


## Gas prices as a coordination mechanism

Overall, users submitting transactions with computation gas prices at or close to the current epoch’s $ReferencePrice$ and storage gas prices at the targeted $StoragePrice$ face good user experience. Sui’s gas price mechanism provides end users with credible reference prices for submitting their transactions. By incentivizing validators to elicit their true reservation prices and honor these quotes, users can credibly assume their transactions will be processed in a timely manner. 

When network activity increases, validators add more workers, increase their costs linearly, and are still able to process transactions at low gas prices. In cases of extreme network congestion where validators cannot scale fast enough, the tip’s presence provides a market-based congestion pricing mechanism that discourages further demand spikes by increasing the cost of transacting on the Sui platform.

In the long run, Sui’s gas mechanism creates incentives for validators to optimize their hardware and operations. Validators who invest in becoming more efficient are able to honor lower gas prices and obtain a stake reward boost. Sui validators are thus encouraged to innovate and improve the experience of end users.


# Sui’s Storage Fund

Sui includes an efficient and sustainable economic mechanism for financing data storage, which is important given Sui’s ability to store arbitrarily large amounts of on-chain data.

Financially, on-chain data storage introduces a severe intertemporal challenge: validators who process and write data into storage today may differ from the future validators needing to store that data. If users were to pay fees for computation power only at write, effectively, future users would need to subsidize past users for their storage and pay disproportionately high fees. This negative network externality can become highly taxing for Sui in the future if left unaddressed.

Sui’s economic design includes a storage fund that redistributes storage fees from past transactions to future validators. When users transact on Sui, they pay fees upfront for both computation and storage. The storage fees are deposited into a storage fund used to adjust the share of future stake rewards distributed to validators relative to SUI delegators. This design is intended to provide future Sui validators with viable business models.


## Storage fund rewards

Sui’s proof-of-stake mechanism calculates total stake as the sum of delegated stake plus the SUI tokens deposited in the storage fund. Hence, the storage fund receives a proportional share of the overall stake rewards depending on its size relative to total stake. The majority of these stake rewards –  a share $\gamma$ – are paid out to current validators to compensate for storage costs while the remaining $(1-\gamma)$ rewards are used to reinvest in the fund. In other words, stake rewards accruing to the storage fees submitted by past transactions are paid out to current validators to compensate them for data storage costs. When on-chain storage requirements are high, validators receive substantial additional rewards to compensate for their storage costs. Vice versa when storage requirements are low. 

More specifically, the storage fund has three key features: 

- It is funded by past transactions and functions as a tool for shifting gas fees across different epochs. This ensures that future validators are compensated for their storage costs by the past users who created those storage requirements in the first place.
- It pays out only the returns on its capital and does not distribute its principal. That is, in practice, it is as if validators were able to borrow the storage fund’s SUI as additional stake and keep the majority of stake rewards (a $\gamma$). But note that validators do not receive funds directly from the storage fund. This guarantees the fund never loses its capitalization and can survive indefinitely. This feature is further buttressed by the $(1-\gamma)$ of stake rewards reinvested in the fund.
- It includes a _deletion option_ by which users obtain a storage fee rebate whenever they delete previously stored on-chain data.[^1] Note that, if a user deletes data, they obtain a partial refund of the storage fees paid originally. This feature is justified by the fact that storage fees exist to pay for storage throughout the data’s lifecycle. There is no reason to keep charging for storage once data is deleted, and so these fees are rebated.

## Storage fund mechanics

The storage fund’s size is fixed throughout each epoch with its size changing at the epoch boundary according to the net inflows accumulated throughout the epoch. Inflows and outflows correspond to:

* Inflows from the storage fees paid for transactions executed during the current epoch.
* Inflows from reinvestments of the fund’s returns into new principal. Specifically, the share $(1-\gamma)\%$ of stake rewards accrued to the storage fund that is not paid out to validators.
* Outflows from storage fee rebates paid to users who delete the data associated with past transactions.

The key property of the rebate function is that it limits storage fund outflows to be always less than the original storage flow, at the individual transaction level. This mechanism guarantees that the storage fund is never depleted and that its size moves in line with the amount of data held in storage.

## Storage fund incentives

The storage fund introduces various desirable incentives into the Sui economy:

* Its mechanics incentivize users to delete data and obtain a rebate on their storage fees when the cost of storing such data exceeds the value obtained from maintaining that data on-chain. This introduces a useful market-based mechanism where users free storage when it becomes uneconomical for them to keep it.
* It creates deflationary pressure over the SUI token in that increased activity leads to larger storage requirements and to more SUI removed from circulation. 
* It is capital efficient in that it is economically equivalent to a rent model where users pay for storage through a pay-per-period model.

# Sui’s Delegated Proof-of-Stake System

The Sui platform relies on delegated proof-of-stake to determine the set of validators who process transactions. 

## SUI token delegation

Within each epoch, operations are processed by a fixed set of validators, each with a specific amount of stake delegated from SUI token holders. A validator's share of total stake is relevant in that it determines each validator’s share of voting power for processing transactions. Delegating SUI implies the SUI tokens are locked for the entire epoch. SUI token holders are free to unstake their SUI or to change their delegate validator when the epoch changes.

## Economic model

We now discuss how the different components of the Sui economy interact with each other in order to introduce Sui’s delegated proof-of-stake system. Throughout, we use the visual representation in the following figure to aid the discussion. 

See the staking and tokenomics diagram in the [Sui Tokenomics](index.md) overview.

The Sui economic model works as follows:

1. At the beginning of each epoch: Three important things happen:
    1. SUI holders delegate (some) of their tokens to validators and a new committee is formed.
    1. The reference gas prices are set as described in Sui’s gas price mechanism.
    1. The storage fund’s size is adjusted using the previous epoch’s net inflow.

    Following these actions, the protocol computes the total amount of stake as the sum of delegated stake plus the storage fund. Call the share of delegated stake alpha%.

1. During each epoch: Users submit transactions to the Sui platform and validators process them. For each transaction, users pay the associated computation and storage gas fees. In cases where users delete previous transaction data, users obtain a partial rebate of their storage fees. Validators observe the behavior of other validators and evaluate each other’s performance.
1. At the end of each epoch: The protocol distributes stake rewards to participants of the proof-of-stake mechanism. This occurs through two main steps:
    1. The total amount of stake rewards is calculated as the sum of computation fees accrued throughout the epoch plus the epoch’s stake reward subsidies. The latter component is optional in that it will disappear in the long run as the amount of SUI in circulation reaches its total supply.
    1. The total amount of stake rewards is distributed across various entities. Importantly, remember that the storage fund is taken into account in the calculation of the epoch’s total stake. However, the storage fund is not owned by any entities in the way that delegated SUI is. Instead, Sui’s economic model distributes the stake rewards accruing to the storage fund  – a share (1-alpha)% of the total stake rewards – to validators in order to compensate them for their storage costs. Of these rewards, a share gamma% is paid out to validators while the remaining (1-gamma)% is used to reinvest in the fund’s capital. Finally, assume that validators charge a commission delta% on SUI token holders as a fee for delegation. The split of stake rewards across participants is given by: 

    	$$ DelegatorRewards \ \ = \ \ (  1 - \delta ) \ \times \  \alpha \ \times \ StakeRewards $$

    	$$ ValidatorRewards \ \ = \ \ ( \ \delta\alpha \ + \ \gamma (1 - \alpha) \ ) \ \times \ StakeRewards $$

    	$$ Reinvestment \ \ = \ \ ( 1 - \gamma ) \ \times \ ( 1 - \alpha ) \ \times \ StakeRewards $$

## Stake reward distribution and incentives

Sui’s gas pricing mechanism together with its delegated proof-of-stake mechanism jointly deliver an efficient economic model whereby validators are encouraged to operate smoothly with low but sustainable gas fees. A specific validator v receives stake rewards equal to:

$$ ValidatorRewards(v) \ \ = \ \ RewardShare(v) \ \times \ ValidatorRewards $$

Where the RewardShare(v) is determined in the gas price mechanism. Note that SUI token holders receive the same share of stake rewards as their delegate validator. Specifically, SUI token holders delegating at a validator v obtain rewards equal to:

$$ DelegatorRewards(v) \ \ = \ \ RewardShare(v) \ \times \ DelegatorRewards $$

On net, this design encourages validators to operate with low gas price quotes – but not too low or else they receive slashed stake rewards. Consequently, Sui’s gas price mechanism and delegated proof-of-stake system encourages a healthy competition for fair prices where validators set low gas fees while operating with viable business models.

## Sui tokenomics conclusion

Sui’s economic model bestows Sui users with an important monitoring role. On the one hand, users want their transactions to be processed as quickly and efficiently as possible. User clients such as wallets encourage this by prioritizing communication with the most responsive validators. Such efficient operations are compensated with boosted rewards relative to less responsive validators. On the other hand, SUI token delegators receive the same boosted or penalized rewards as their delegate validator. An unresponsive validator is thus doubly exposed to Sui’s incentives: they lose directly through slashed rewards and indirectly through reduced delegated stake in future epochs as stakers move their tokens to more responsive validators. 

<!-- Footnotes themselves at the bottom. -->

## Notes

[^1]:

     This should not be confused with deleting past transactions. Activity on Sui is finalized at each epoch boundary and thus past transactions are immutable and can never be reversed. The type of data that can be deleted is, for example, data corresponding to objects that are no longer live such as an NFT’s metadata, tickets that have been redeemed, auctions that have concluded, etc.
