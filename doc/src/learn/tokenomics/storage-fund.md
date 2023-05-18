---
title: Sui’s Storage Fund
---

Sui includes an efficient and sustainable economic mechanism for financing data storage, which is important given Sui’s ability to store arbitrarily large amounts of on-chain data.

Financially, on-chain data storage introduces a severe inter-temporal challenge: validators who process and write data into storage today may differ from the future validators needing to store that data. If users were to pay fees for computation power only at write, effectively, future users would need to subsidize past users for their storage and pay disproportionately high fees. This negative network externality can become highly taxing for Sui in the future if left unaddressed.

Sui’s economic design includes a storage fund that redistributes storage fees from past transactions to future validators. When users transact on Sui, they pay fees upfront for both computation and storage. The storage fees are deposited into a storage fund used to adjust the share of future stake rewards distributed to validators relative to the users that stake SUI with them. This design is intended to provide future Sui validators with viable business models.

## Storage fund rewards

Sui’s Delegated Proof-of-Stake mechanism calculates total stake as the sum of user stake plus the SUI tokens deposited in the storage fund. Hence, the storage fund receives a proportional share of the overall stake rewards depending on its size relative to total stake. The majority of these stake rewards –  a share $\gamma$ – are paid out to current validators to compensate for storage costs while the remaining $(1-\gamma)$ rewards are used to reinvest in the fund. In other words, stake rewards accruing to the storage fees submitted by past transactions are paid out to current validators to compensate them for data storage costs. When on-chain storage requirements are high, validators receive substantial additional rewards to compensate for their storage costs. Vice versa when storage requirements are low. 

More specifically, the storage fund has three key features:
* The storage fund is funded by past transactions and functions as a tool for shifting gas fees across different epochs. This ensures that future validators are compensated for their storage costs by the past users who created those storage requirements in the first place.
* The storage fund pays out only the returns on its capital and does not distribute its principal. That is, in practice, it is as if validators were able to borrow the storage fund’s SUI as additional stake and keep the majority of stake rewards (a share $\gamma$). But note that validators do not receive funds directly from the storage fund. This guarantees the fund never loses its capitalization and can survive indefinitely. This feature is further buttressed by the share $(1-\gamma)$ of stake rewards reinvested in the fund.
* The storage fund includes a _deletion option_ by which users obtain a storage fee rebate whenever they delete previously stored on-chain data. Note that, if a user deletes data, they obtain a partial refund of the storage fees paid originally. This feature is justified by the fact that storage fees exist to pay for storage throughout the data’s lifecycle. There is no reason to keep charging for storage once data is deleted, and so these fees are rebated.

**Important:** The _deletion option_ should not be confused with deleting past transactions. Activity on Sui is finalized at each epoch boundary and  past transactions are immutable and can never be reversed. The type of data that can be deleted is, for example, data corresponding to objects that are no longer live such as an NFT’s metadata, tickets that have been redeemed, auctions that have concluded, etc.

## Storage fund mechanics

The storage fund’s size is fixed throughout each epoch with its size changing at the epoch boundary according to the net inflows accumulated throughout the epoch. Inflows and outflows correspond to:
* Inflows from the storage fees paid for transactions executed during the current epoch.
* Inflows from reinvestments of the fund’s returns into new principal. Specifically, the share $(1-\gamma)$ of stake rewards accrued to the storage fund that is not paid out to validators.
* Outflows from storage fee rebates paid to users who delete the data associated with past transactions.

The key property of the rebate function is that it limits storage fund outflows to be always less than the original storage flow, at the individual transaction level. This mechanism guarantees that the storage fund is never depleted and that its size moves in line with the amount of data held in storage.

## Storage fund incentives

The storage fund introduces various desirable incentives into the Sui economy:
* Its mechanics incentivize users to delete data and obtain a rebate on their storage fees when the cost of storing such data exceeds the value obtained from maintaining that data on-chain. This introduces a useful market-based mechanism where users free storage when it becomes uneconomical for them to keep it.
* It creates deflationary pressure over the SUI token in that increased activity leads to larger storage requirements and to more SUI removed from circulation. 
* It is capital efficient in that it is economically equivalent to a rent model where users pay for storage through a pay-per-period model.
