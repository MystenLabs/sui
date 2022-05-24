---
title: Sui’s Gas-Pricing Mechanism
---

Sui’s gas-pricing mechanism achieves the triple outcomes of delivering users with low, predictable transaction fees, of incentivizing validators to optimize their transaction processing operations, and of preventing denial of service attacks. 

This delivers good user experience to Sui users, who can focus on using the Sui network without worrying about having to forecast the current market price of gas fees. Since validators agree on a network-wide reference price at the start of each epoch, Sui users use the reference price as a credible anchor when submitting their transactions. Moreover, the price setting mechanism is designed to reward good validator behavior, thus aligning incentives between SUI token holders, the network’s operators (i.e. the validators), and its users.

A unique feature of Sui’s gas price mechanism is that users pay separate fees for transaction execution and for storing the data associated with each transaction. The gas fees associated with an arbitrary transaction $\tau$ equal:

$GasFees[\tau] \ = \ ComputationUnits[\tau] \times ComputationPrice[\tau] \ + \ StorageUnits[\tau] \times StoragePrice$

The gas functions $ComputationUnits[\tau]$ and $StorageUnits[\tau]$ measure the amount of computation and storage resources, respectively, required to process and store the data associated with $\tau$. The gas prices $ConsumptionPrice[\tau]$ and $StoragePrice$ translate the cost of computation and storage, respectively, into SUI units. The decoupling between gas units and gas prices is useful since SUI’s market price will fluctuate over time depending on demand and supply fluctuations.

## Computation gas prices

The computation gas price $ComputationPrice[\tau]$ captures the cost of one unit of computation in SUI units. This price is set at the transaction level and submitted by the user in two parts:

$ComputationPrice[\tau] \ = \ ReferencePrice \ + \ Tip[\tau]$  

$\text{with } \ ComputationPrice[\tau] > PriceFloor$

The $ReferencePrice$ is fixed at the network level for the duration of the epoch, while the tip is at the user’s discretion. Since the tip can be negative, in practice the user can submit any gas price – as long as the overall price is higher than the $PriceFloor$. Note that the $PriceFloor$ will be updated periodically, with $ReferencePrice > PriceFloor$, and exists mainly to prevent a flood of network spamming.

Sui’s gas price mechanism is intended to make the $ReferencePrice$ a credible anchor for users to use when submitting transactions to the network, thereby providing reasonable confidence that transactions submitted with gas prices at or close to the reference price will be executed in a timely manner. This is achieved through three core steps:
* _Gas Price Survey_ - A validatory-wide survey is conducted at the beginning of each epoch, and every validator submits their reservation price. That is, each validator states the minimum gas price at which they are willing to process transactions. The protocol orders these quotes and chooses the 2/3's percentile by stake as the reference price. The gas price survey’s goal is to set a reference price under which a [quorum](../architecture/validators.md#quorums) of validators are willing to promptly process transactions.
* _Tallying Rule_ - Throughout the epoch, validators obtain signals over the operations of other validators. Each validator uses these signals to build a (subjective) evaluation over the performance of every other validator. Specifically, each validator constructs a multiplier for the stake rewards of every other validator such that validators who behave well receive boosted rewards, and validators who do not receive reduced rewards. Good behavior is proxied by the share of transactions above a validator’s self-declared reservation price that the validator processed in a timely manner. The tallying rule’s goal is to create a community-enforced mechanism for encouraging validators to honor the quotes submitted during the gas survey.
* _Incentivized Stake Reward Distribution Rule_ - At the end of the epoch, the distribution of stake rewards across validators is adjusted using information from the gas price survey and tallying rule. Specifically, a global multiplier is built for every validator using the median value – weighted by stake – out of the set of individual multipliers constructed during the tallying rule. The incentivized stake reward distribution then sets the share of stake rewards distributed to each validator $v$ as:

$ RewardShare(v) = Constant \times (1 + GasSurveyBoost) \times Multiplier(v) \times StakeShare(v) $

The $Constant$ term is used as a normalization such that the sum of $RewardShare(v)$ across the validator set sums up to one. If the validator submitted a price quote under the $ReferencePrice$, then $GasSurveyBoost > 0$. If not, $GasSurveyBoost < 0$. The purpose of this booster is to encourage validators to submit low reservation prices during the gas price survey. Finally, $Multiplier(v)$ is the global multiplier built from the subjective evaluations in the tallying rule. Note that in a symmetric equilibrium where all validators submit the same quote to the gas price survey and where all validators behave well as measured by the tallying rule, then $ RewardShare(v) = StakeShare(v)$ and each validator receives stake rewards in proportion to their share of overall stake.

In sum, the gas price mechanism has two main forces: the tallying rule incentivizes validators to honor the quotes submitted during the gas survey, while the distribution rule incentivizes validators to submit low reservations prices. The interaction of these two forces delivers a mechanism encouraging validators to set a low network-level reference gas price – but not too low since they face penalties if they cannot honor their quotes. In other words, the gas price mechanism encourages a healthy competition for fair prices.


## Storage gas prices

The storage gas price $StoragePrice$ captures the costs of covering one unit of storage in perpetuity, in SUI units. This price is set through governance proposals and is updated infrequently. The goal is to ensure Sui users pay for their use of on-chain data storage by depositing these fees into the storage fund and then redistributing these fees to future validators. In contrast to the computation gas price, storage prices are fixed and common for all transactions both within an epoch and across epochs until the storage price is updated.

The $StoragePrice$ is set exogenously through the governance proposal with the goal of targeting the off-chain dollar cost of data storage. In the long run, as the costs of storage fall due to technological improvements and the dollar price of the SUI token evolves, governance proposals will update the price in order to reflect the new dollar target price.


## Gas prices as a coordination mechanism

Overall, users submitting transactions with computation gas prices at or close to the current epoch’s $ReferencePrice$ and storage gas prices at the targeted $StoragePrice$ face good user experience. Sui’s gas price mechanism provides end users with credible reference prices for submitting their transactions. By incentivizing validators to elicit their true reservation prices and honor these quotes, users can credibly assume their transactions will be processed in a timely manner. 

When network activity increases, validators add more workers, increase their costs linearly, and are still able to process transactions at low gas prices. In cases of extreme network congestion where validators cannot scale fast enough, the tip’s presence provides a market-based congestion pricing mechanism that discourages further demand spikes by increasing the cost of transacting on the Sui platform.

In the long run, Sui’s gas mechanism creates incentives for validators to optimize their hardware and operations. Validators who invest in becoming more efficient are able to honor lower gas prices and obtain a stake reward boost. Sui validators are thus encouraged to innovate and improve the experience of end users.
