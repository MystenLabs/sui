---
title: Sui Gas Budgets
---

Gas Budgets are important because they guarantee that an user never pays more transaction fees than what they budget for. Sui’s gas mechanism is such that users pay upfront for both the current computational cost of executing a transaction and the long-term cost of storing the data associated with a transaction’s objects. Thus, Gas Budgets need to cover both sets of fees.

Specifically, [Sui’s Gas Pricing Mechanism](https://docs.sui.io/learn/tokenomics/gas-pricing) is such that any transaction $\tau$ pays the following gas fees:

$$
\text{GasFees}[\tau] = \text{ComputationUnits}[\tau]\times \text{ReferenceGasPrice}\ + \ \text{StorageUnits}[\tau]\times \text{StoragePrice}
$$

While computation and storage fees are separate, they are conceptually similar in that they each translate computation or storage into SUI terms by multiplying computation or storage units by the relevant price. 

### Gas Prices

The [Reference Gas Price](gas-pricing.md#computation-gas-prices) translates the real-time cost of executing a transaction into SUI units and is updated at each epoch boundary by the validator set. Similarly, the [Storage Price](gas-pricing.md#storage-gas-prices)  translates the long-term cost of storing data on-chain into SUI units and is updated infrequently; often remaining constant for various consecutive epochs. During regular network operations, all Sui users should expect to pay the Reference Gas Price and Storage Price for computation and storage, respectively.

### Gas Units

Different Sui transactions require varying amounts of computational time in order to be processed and executed. Sui translates these varying operational loads into transaction fees by measuring each transaction in terms of Computation Units. All else equals, more complex transactions will require more Computation Units. 

Importantly, though, Sui’s gas schedule is built coarsely with a bucketing approach. Two relatively similar transactions will translate into the exact same amount of Computation Units if they are in the same bucket, whereas two relatively different transactions will translate into different amounts of Computation Units if they fall in separate buckets. The smallest bucket maps into 1000 Computation Units, meaning that all transactions that fall into the smallest bucket will cost 1000 Computation Units. The largest bucket maps into 5,000,000 Computation Units; if a transaction were to require more Computation Units it would simply abort.

Similarly, Sui transactions vary depending on the amount of new data written into on-chain storage. The variable Storage Units captures these difference by mapping the amount of bytes held in storage into storage units. Sui’s current schedule is linear and maps each byte into 100 storage units. So, for example, a transaction that stores 25 bytes will cost 2500 Storage Units while a transaction that stores 75 bytes will cost 7500 units.

### Gas Budgets

All transactions need to be submitted together with a Gas Budget. This provides a cap to the amount of Gas Fees a user will pay, especially since in some cases it may be hard to perfectly forecast how much a transaction will cost before it is submitted to the Sui Network.

A transaction’s Gas Budget is defined in SUI units and transactions will be successfully executed if:

$$
\text{GasBudget}[\tau]\ \geq\ \text{GasFees}[\tau]
$$

If the Gas Budget does not fulfill this condition — and thus is insufficient to cover a transaction’s gas fees — then the transaction will fail and the entire Gas Budget will be charged. 

The minimum Gas Budget is 2000 MIST. This ensures validators can be compensated with at least 2000 MIST even if the Gas Budget is mis-specified and transaction aborts. Additionally, this protects the Sui Network from being spammed with a large number of transactions with minimal gas budgets. The maximum Gas Budget is 50 billion MIST or 50 SUI. This protects the network against overflow of internal multiplications and gas limits for denial of service attack

Finally, Sui’s [Storage mechanics](https://docs.sui.io/learn/tokenomics/storage-fund#storage-fund-rewards) provide storage fee rebates whenever a transaction deletes previously-stored objects. Hence, the net fees that a user pays equals Gas Fees minus the rebates associated with data deletion:

$$
\text{NetGasFees}[\tau]\ =\ \text{GasFees}[\tau]\ -\text{StorageRebate}[\tau]
$$

Since the Gas Budget applies to the totality of Gas Fees, it will often be the case that a transaction will only go through if the Gas Budget is considerably higher than the Net Gas Fees that a user ultimately pays.

### Gas Budget Examples

The following table provides some examples of gas accounting in the Sui network. Note that within the first two rows and within the last two rows, Computation Units are the same because transactions fall within the same buckets. However, the last two transactions are more complex than the first two and thus fall in a higher bucket. Finally, note that in the last transaction the storage rebate is large enough to fully offset the transactions Gas Fees and actually pays the user back a positive amount of SUI. 

These examples also showcase the importance of the Gas Budget. The minimum Gas Budget is the smallest amount a transaction can specify in order to successfully execute. Importantly, note that when there is a storage rebate, the minimum Gas Budget is larger than the amount of Net Gas Fees a user ultimately pays — this is especially stark in the last example where the user receives a positive amount back for executing the transaction. This is because the minimum Gas Budget is applied to the raw Gas Fees value, not including the Storage Rebate.

|  | Reference Gas Price | Computation Units | Storage Price | Storage Units | Storage Rebate | Minimum Gas Budget | Net Gas Fees |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Simple transaction storing 10 bytes | 1,000 MIST | 1,000 | 75 MIST | 1,000 | 0 MIST | 1,075,000 MIST | 1,075,000 MIST |
| Simple transaction storing 10 bytes and deleting data | 500 MIST | 1,000 | 75 MIST | 1,000 | 100,000 MIST | 575,000 MIST | 475,000 MIST |
| Complex transaction storing 120 bytes | 1,000 MIST | 5,000 | 200 MIST | 12,000 | 0 MIST | 7,400,000 MIST | 7,400,000 MIST |
| Complex transaction storing 120 bytes and deleting data | 500 MIST | 5,000 |  200 MIST | 12,000 | 5,000,000 MIST | 2,400,000 MIST | -100,000 MIST |
