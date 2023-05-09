---
title: Gas in Sui
---

A Sui transaction must pay for both the computational cost of execution and the long-term cost of storing the objects a transaction creates or mutates. Specifically, [Sui’s Gas Pricing Mechanism](gas-pricing.md) is such that any transaction pays the following gas fees:

`total_gas_fees = computation_units * reference_gas_price + storage_units * storage_price`

While computation and storage fees are separate, they are conceptually similar in that they each translate computation or storage into SUI terms by multiplying computation or storage units by the relevant price. 

Finally, Sui’s [Storage mechanics](https://docs.sui.io/learn/tokenomics/storage-fund#storage-fund-rewards) provide storage fee rebates whenever a transaction deletes previously-stored objects. Hence, the net fees that a user pays equals Gas Fees minus the rebates associated with data deletion:

`net_gas_fees = computation_gas_fee + storage_gas_fee - storage_rebate`

The information on net gas fees displays in [Sui Explorer](https://suiexplorer.com/) for each transaction block:

![Gas Fees displayed on Sui Explorer](../../static/gas-fees-explorer.png "The Gas Fees section displayed on Sui Explorer")
*The Gas Fees section for a transaction block displayed on Sui Explorer*

### Gas Prices

The [Reference Gas Price](https://docs.sui.io/learn/tokenomics/gas-pricing#computation-gas-prices) translates the real-time cost of executing a transaction into SUI units and is updated at each epoch boundary by the validator set. Similarly, the [Storage Price](https://docs.sui.io/learn/tokenomics/gas-pricing#storage-gas-prices)  translates the long-term cost of storing data on-chain into SUI units and is updated infrequently; often remaining constant for various consecutive epochs. During regular network operations, all Sui users should expect to pay the Reference Gas Price and Storage Price for computation and storage, respectively.

### Gas Units

**Computation Units**

Different Sui transactions require varying amounts of computational time in order to be processed and executed. Sui translates these varying operational loads into transaction fees by measuring each transaction in terms of Computation Units. All else equals, more complex transactions will require more Computation Units.

Importantly, though, Sui’s computation gas schedule is built coarsely with a bucketing approach. Two relatively similar transactions will translate into the exact same amount of Computation Units if they are in the same bucket, whereas two relatively different transactions will translate into different amounts of Computation Units if they fall in separate buckets. The smallest bucket maps into 1,000 Computation Units, meaning that all transactions that fall into the smallest bucket will cost 1,000 Computation Units. The largest bucket maps into 5,000,000 Computation Units; if a transaction were to require more Computation Units it would simply abort.

Using coarse bucketing accomplishes two important goals:
* Frees developers from optimizing their smart contracts to deliver marginal gains in gas costs via "gas golfing" — instead, can focus on step-function improvements in their products and services.
* Gives Sui protocol devs the freedom to adjust per-instruction gas costs and experiment with new gas metering schemes without creating significant disruption for builders. This can happen frequently, so it's important that builders do *not* rely on per-instruction gas costs remaining stable over time.

| Bucket Lower Threshold | Bucket Upper Threshold | Computation Units |
| --- | --- | --- |
| 0 | 1,000 | 1,000 |
| 1,001 | 5,000 | 5,000 |
| 5,001 | 10,000 | 10,000 |
| 10,001 | 20,000 | 20,000 |
| 20,001 | 50,000 | 50,000 |
| 50,001 | 200,000 | 200,000 |
| 200,001 | 1,000,000 | 1,000,000 |
| 1,000,001 | 5,000,000 | 5,000,000 |
| 5,000,001 | Infinity | transaction will abort |

**Storage Units**

Similarly, Sui transactions vary depending on the amount of new data written into on-chain storage. The variable Storage Units captures these difference by mapping the amount of bytes held in storage into storage units. Sui’s current schedule is linear and maps each byte into 100 storage units. So, for example, a transaction that stores 25 bytes will cost 2,500 Storage Units while a transaction that stores 75 bytes will cost 7,500 units.

Importantly, in Sui’s [Storage Fund](https://docs.sui.io/learn/tokenomics/storage-fund) model users pay upfront for the cost of storing data in perpetuity but can also get a partial rebate on previously stored data if that data is deleted. Hence, the amount of storage fees that a user pays can be split into a rebateable and non-rebateable amount. Initially, the rebateable amount equals 99% of the storage fees while the non-rebateable amount equals 1% of the storage fee.

### Gas Budgets

All transactions need to be submitted together with a Gas Budget. This provides a cap to the amount of Gas Fees a user will pay, especially since in some cases it may be hard to perfectly forecast how much a transaction will cost before it is submitted to the Sui Network. 

A transaction’s Gas Budget is defined in SUI units and transactions will be successfully executed if:

`gas_budget >= max{computation_fees,total_gas_fees}`

If the Gas Budget does not fulfill this condition then the transaction will fail and a portion of the Gas Budget will be charged. In cases where the `gas_budget` is insufficient for covering `computation_fees`, then the entirety of the `gas_budget` will be charged. In cases where `gas_budget` is sufficient for covering `computation_fees` but not the `total_gas_fees`, then a portion of the `gas_budget` corresponding to `computation_fees` and the fees associated with mutating the transaction's input objects will be charged.

Ultimately, a successful transaction will require the end user to pay the transaction's `total_gas_fees`. However, since it is challenging to perfectly forecast computation time before the transaction is processed, the `gas_budget` condition also requires the `gas_budget` to be at least as large as the transaction's `computation_fees` in case the transaction aborts. In some cases -- especially in the presence of high storage rebates, and, thus negative net storage fees -- the Gas Budget may be higher than the total gas fees paid by the end user.

Importantly, the minimum Gas Budget is 2,000 MIST. This ensures validators can be compensated with at least 2,000 MIST even if the Gas Budget is incorrectly specified and the transaction aborts. Additionally, this protects the Sui Network from being spammed with a large number of transactions with minimal gas budgets. The maximum Gas Budget is 50 billion MIST or 50 SUI. This protects the network against overflow of internal multiplications and gas limits for denial of service attack.

As mentioned above, the Storage Rebate currently equals 99% of the originally paid Storage Fees. Since the Gas Budget applies to the totality of Gas Fees, it will often be the case that a transaction will only go through if the Gas Budget is considerably higher than the Net Gas Fees that a user ultimately pays.

### Gas Budget Examples

The following table provides some examples of gas accounting in the Sui network. Note that within the first two rows and within the last two rows, Computation Units are the same because transactions fall within the same buckets. However, the last two transactions are more complex than the first two and thus fall in a higher bucket. Finally, note that in the last transaction the storage rebate is large enough to fully offset the transactions Gas Fees and actually pays the user back a positive amount of SUI. 

These examples also showcase the importance of the Gas Budget. The minimum Gas Budget is the smallest amount a transaction can specify in order to successfully execute. Importantly, note that when there is a storage rebate, the minimum Gas Budget is larger than the amount of Net Gas Fees a user ultimately pays — this is especially stark in the last example where the user receives a positive amount back for executing the transaction. This is because the minimum Gas Budget must be higher than a transaction's Computation Fees.

|  | Reference Gas Price | Computation Units | Storage Price | Storage Units | Storage Rebate | Minimum Gas Budget | Net Gas Fees |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Simple transaction storing 10 bytes | 1,000 MIST | 1,000 | 75 MIST | 1,000 | 0 MIST | 1,075,000 MIST | 1,075,000 MIST |
| Simple transaction storing 10 bytes and deleting data | 500 MIST | 1,000 | 75 MIST | 1,000 | 100,000 MIST | 500,000 MIST | 475,000 MIST |
| Complex transaction storing 120 bytes | 1,000 MIST | 5,000 | 200 MIST | 12,000 | 0 MIST | 7,400,000 MIST | 7,400,000 MIST |
| Complex transaction storing 120 bytes and deleting data | 500 MIST | 5,000 |  200 MIST | 12,000 | 5,000,000 MIST | 2,500,000 MIST | -100,000 MIST |
