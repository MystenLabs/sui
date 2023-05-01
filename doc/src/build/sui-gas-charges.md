---
title: Sui Gas Fees
---

To learn more about gas fees in the context of Sui Tokenomics, see [Gas in Sui](../learn/tokenomics/gas-in-sui.md).

This topic provides a summary of the changes to the Sui Gas model for Sui Mainnet.

## Transaction Input: GasData

A transaction takes a `GasData` structure that defines all information about the gas provided for a transaction.

```rust
pub struct GasData {
	pub payment: Vec<ObjectRef>,
	pub owner: SuiAddress,
	pub price: u64,
	pub budget: u64,
}
```

- `payment` is a vector of gas coin. It accepts only SUI coins, and the coins must be unique. On execution, Sui merges the coins together into a single coin object, specifically the first one. After execution, Sui deletes all of the coins except the first coin that holds the balance of the coins combined minus the gas cost for the transaction.
- `owner` is the single owner of all coins used, either the sender, or the sponsor for [sponsored transactions](../learn/sponsored-transactions.md).
- `price` defines how much users will pay for a single unit of execution. A unit of execution is an abstract concept internal to the implementation. A higher price uses more gas than a lower price. At a later point, `price` helps define the priority of executing the transaction. The value for `price` must be greater than or equal to the [Reference Gas Price](../learn/tokenomics/gas-pricing.md), which is the minimum amount the system accepts for a transaction.
- `budget` is the maximum amount a user pays for a transaction. The value for `budget` is expressed in MIST, a fractional unit of SUI. Each MIST equals 10^-9 SUI - 1 SUI equals 1 Billion MIST. The sum of all coins in `payment` must be greater than or equal to the amount specified for `budget`.

Note that the current release handles `payment` and `budget` dofferently than previous releases (prior to release .28).

Computation has a max computation budget of `5_000_000` units and any `budget` value that is higher than that (`5_000_000 * gas_price`) counts exclusively towards storage. The max computation budget should be enough to account for very complex transactions.

Merging of gas coins (known as “gas smashing”) is performed irrespective of the result of the transaction. Even on failures and out of gas errors (`InsufficientGas`), gas coin are merged and all but the first one is deleted.

System values can (and will) change over time. Indexers, wallets and full nodes should be able to provide information on what those values are at any time.

## Budget Consumption

Gas usage is proportional to the resources used. Specifically, it is proportional to **execution**, in terms of computation and memory, and **DB access**, in terms of Sui object reads and writes.

There are 2 main values defined in the system that contribute to gas consumption:

- **RGP** (Reference Gas Price): set up at the beginning of each epoch by the validator committee. RGP is a multiplier applied to each unit of computation. Initial **RGP** at genesis is 1,000 MIST.
- **Storage Price**: set up in the system and intended to be more stable than RGP, it is a multiplier for the number of bytes written to storage. Initial **Storage Price** at genesis is **76** MIST.

## Transaction Output: GasCostSummary

On transaction output, Sui provides the following information about gas:

```rust
pub struct GasCostSummary {
	pub computation_cost: u64,
	pub storage_cost: u64,
	pub storage_rebate: u64,
	pub non_refundable_storage_fee: u64,
}
```

The final gas charge, and so the amount subtracted from the gas coin (the first one after “gas smashing”) is:
`gas charge = computation_cost + storage_cost - storage_rebate`

The `computation_cost` is the execution charge, defined as computation and memory cost.

The remaining values provide an insight into the storage charges. Each time Sui writes to or reads from an object in the database, the object is conceptually _deleted_ and _restored_ to the DB. A transaction could also delete objects and create new objects.

At the end of execution, all created object contribute to `storage_cost` with the following formula: `storage_cost = object_byte * storage_normalizer * storage_price` where, currently, `storage_normalizer = 100` and `storage_price = 76`. The `storage_cost` is saved and tracked by each object, and represents the value of the object in terms of its storage cost.

All deleted objects storage values are added together and refunded to the user (`storage_rebate`), except for a small percentage of the rebate that is charged by the system and goes into the `non_refundable_storage_fee`. That percentage is currently defined to be 0.01%.

## Out Of Gas Model

*Out Of Gas* is possibly the most challenging aspect of gas charging. The gas charges model is well defined when there is enough gas for all operations in a transaction.

When *Out Of Gas* happens, charging varies depending on where the out of gas occurs. Consider the following scenarios:

1. Out of gas in execution, all effects are reverted. **However** there is still a charge for storage as input objects are accessed. If there is any storage budget left (gas budget higher than max computation budget) Sui uses that budget, charges for storage of the input objects, and returns an **Insufficient Gas** (go to step 3). On success, `GasCostSummary` has all fields greater than 0 and reflects the true cost of the transaction.
2. Execution is successful, but charging for storage causes *Out of Gas*.
   The effects are reverted, storage charges are zeroed, and then performed again for the input objects (go to step 3). On success `GasCostSummary` has all fields greater than 0 and reflects the true cost of the transaction.
   Note that this behavior described will be implemented before Sui Mainnet, but is not currently supported. As of release .30, *Out Of Gas* occurrence while charging storage collects the entire budget and assigns it to `computation_cost`. The transaction aborts with an `InsufficientGas` error.
3. If storage charges for input objects fail, Sui gets all of the budget assigned to `computation_cost`, and leaves the object storage value unchanged. In this case, the values related to storage in the `GasCostSummary` are all zeroed and the `computation_cost` is the only value set in `GasCostSummary`. The transaction aborts with an `InsufficientGas`.
