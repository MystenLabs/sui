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

![Sui tokenomics flow](../../../static/sui-tokenomics-flow.png "See staking and tokenomics in Sui")
*Visualize staking and tokenomics in Sui*

# The SUI token

Sui’s native asset is called SUI and we generally use the capitalized version of SUI to distinguish the token from the Sui platform.

The total supply of SUI is capped at 10,000,000,000 (i.e. ten billion tokens). A share of SUI’s total supply will be liquid at mainnet launch, with the remaining tokens vesting over the coming years or distributed as future stake reward subsidies. Each SUI token is divisible up to a large number of decimal places.

The SUI token serves four purposes on the Sui platform:

* SUI can be staked within an epoch in order to participate in the proof-of-stake mechanism. 
* SUI is the asset denomination needed for paying the gas fees required to execute and store transactions or other operations on the Sui platform. 
* SUI can be used as a versatile and liquid asset for various applications including the standard features of money – a unit of account, a medium of exchange, or a store of value – and more complex functionality enabled by smart contracts, interoperability, and composability across the Sui ecosystem. 
* SUI token plays an important role in governance by acting as a right to participate in on-chain voting on issues such as protocol upgrades.

Since the SUI token is available in finite supply, SUI may face deflationary pressure in the long run if Sui unlocks more use cases and more users migrate to the platform. In addition, the storage fund’s presence creates an additional deflationary force in that higher on-chain data requirements translate into a larger storage fund, thus reducing the amount of SUI in circulation.

Continue learning about Sui tokenomics with our:
1. [Gas-pricing mechanism](../tokenomics/gas-pricing.md).
1. [Sui storage fund](../tokenomics/storage-fund.md).
2. [Delegated proof-of-stake system](../tokenomics/proof-of-stake.md).

