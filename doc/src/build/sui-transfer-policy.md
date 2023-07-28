---
title: Sui Transfer Policy
---

A Sui Transfer Policy ([`TransferPolicy` object](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/docs/transfer_policy.md) specifies the conditions that must be met to transfer an item from a Sui kiosk. You must include a transfer policy to enable transfers, and the policy must be available to other users for them to purchase an item from your kiosk. When a transaction occurs on an item in a kiosk, the buyer receives a transfer request (`TransferRequest` struct). The transfer request must be confirmed in the associated transfer policy for the transaction to succeed.

By default, a transfer policy is empty, and the request confirmation does not require any actions from the buyer other than calling the `confirm_request` function. However, as the owner of the transfer policy (`TransferPolicy` object), you can add any number of programmable rules that  must be completed for the request to be confirmed.

**Transfer policies are:**
 * Created using the `Publisher` object authorization
 * Issued per type, and use only this type
 * Enforced on all trades that occur in a kiosk
 * Required for a type (T) to be made available for sale in a kiosks

## Create a transfer policy

A transfer policy applies to a specified type `<T>`, which means that for every Move type you define, you need to also create a separate `TransferPolicy` object. To create one, you call the `transfer_policy::new` function. You can also use the `transfer_policy::default` function to create a basic default policy. Kiosk transfer policy authorization is performed via the `Publisher` object - if you defined and published the type `<T>`, then you are authorized to set the `TransferPolicy` for the type `<T>`.

When you create (and share) a `TransferPolicy`, it emits a special event that makes the policy  discoverable on the network.

## Transfer policy rules

A default, empty `TransferPolicy` does not require any action from the buyer for the transaction to succeed. You can, however, implement custom rules for the transfer policy. A rule is a Move module published on chain, and usually has two functions: one for the creator to add and configure the rule, and another that allows the buyer to perform certain actions and get a rule receipt.

After you add a rule to a transfer policy, all transactions from the kiosk for items that use the associated type must complete the conditions specified in the rule. Confirmation is provided in the receipt attached by the `Rule` module. 

## Custom transfer policy rules

While Sui supports and allows for you to add any rules to a transfer policy, the Sui ecosystem needs to implement support for each rule independently. Hence, adding new rules is a tricky process which requires ecosystem agreement. For example, you create a transfer policy for a type `MyHeroes`. You then add a transfer policy that includes a rule that any buyer must complete Know Your Customer (KYC) verification, and to perform KYC checks you implement custom logic. Other marketplaces and wallets on the network are not aware of the rule you added to your transfer policy. The purchase process can’t be automated, and no purchase of an asset with a type of `MyHeroes` can be completed.

### TransferPolicy code samples

Create a policy for T; should be shared or frozen so it is available:
```rust
transfer_policy::new<T>(&Publisher): (TransferPolicy, TransferPolicyCap)
```

Confirm and resolve the request; transaction can be completed:
```rust
transfer_policy::confirm_request<T>(TransferPolicy, TransferRequest<T>)
```
