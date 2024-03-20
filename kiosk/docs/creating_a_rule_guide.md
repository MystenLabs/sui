# Creating a Rule: Guide

When an item is purchased in a Kiosk, a TransferRequest potato is created, and the only way to resolve it and unblock the transaction is to confirm the request in the matching TransferPolicy. This guide explains how TransferPolicy works and how new rules can be implemented and added into a policy.

## Basics

An item of a type T can only be traded in Kiosks if the TransferPolicy for T exists and available to the buyer. This requirement is based on a simple fact that the TransferRequest issued on purchase must be resolved in a matching TransferPolicy and if there isn't one or buyer can't access it, the transaction will fail.

This system was designed to give maximum freedom and flexibility for creators: by taking the transfer policy logic out of the trading primitive we make sure that the policies can be set only by creators, and as long as the trading primitive is used, enforcements are under their control. Effectively creators became closer to the trading ecosystem and got an important and solid role in the process.

## Architecture

By default, a single TransferPolicy does not enforce anything - if a buyer attempts to confirm their TransferRequest, it will go through. However, the system allows setting so-called "Rules". Their logic is simple: someone can publish a new rule module, for example "fixed fee", and let it be "added" or "set" for the TransferPolicy. Once the Rule is added, TransferRequest needs to collect a TransferReceipt marking that the requiement specified in the Rule was completed.

\[TODO\]

- Once a Rule is added to the TransferPolicy, every TransferRequest going to the policy must have a matching Receipt

## Rule structure: Dummy

Every rule would follow the same structure and implement required types:

1. RuleWitness struct
2. Config struct stored in the TransferPolicy
3. "set" function which adds the Rule to the TP
4. an action function which adds a Receipt to the TransferRequest

> Important: there's no need to implement "unset" - any rule can be removed at any time as defined in the TransferPolicy module and guaranteed by the set of constraints on the rule Config (store + drop)

```move
module examples::dummy_rule {
    use sui::coin::Coin;
    use sui::sui::SUI;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// The Rule Witness; has no fields and is used as a
    /// static authorization method for the rule.
    struct Rule has drop {}

    /// Configuration struct with any fields (as long as it
    /// has `drop`). Managed by the Rule module.
    struct Config has store, drop {}

    /// Function that adds a Rule to the `TransferPolicy`.
    /// Requires `TransferPolicyCap` to make sure the rules are
    /// added only by the publisher of T.
    public fun set<T>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>
    ) {
        policy::add_rule(Rule {}, policy, cap, Config {})
    }

    /// Action function - perform a certain action (any, really)
    /// and pass in the `TransferRequest` so it gets the Receipt.
    /// Receipt is a Rule Witness, so there's no way to create
    /// it anywhere else but in this module.
    ///
    /// This example also illustrates that Rules can add Coin<SUI>
    /// to the balance of the TransferPolicy allowing creators to
    /// collect fees.
    public fun pay<T>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: Coin<SUI>
    ) {
        policy::add_to_balance(Rule {}, policy, payment);
        policy::add_receipt(Rule {}, request);
    }
}
```

This module contains no configuration and requires a `Coin<SUI>` of any value (even "0"), so it's easy to imagine that every buyer would create a zero Coin and pass it to get the Receipt. The only thing this Rule module is good for is illustration and a skeleton. Goes without saying but *this code should never be used in production*.

## Reading the Request: Royalty

To implement a percentage-based fee (a very common scenario - royalty fee), a Rule module needs to know the price for which an item was purchased. And the TransferRequest contains some information which can be used in this and other scenarios:

1. Item ID
2. Amount paid (SUI)
3. From ID - the object which was used for selling (eg Kiosk)

> To provide access to these fields, the `sui::transfer_policy` module has a set of getter functions which are available to anyone: "paid()", "item()" and "from()"

```move
module examples::royalty_rule {
    // skipping dependencies
    const MAX_BP: u16 = 10_000;

    struct Rule has drop {}

    /// In this implementation Rule has a configuration - `amount_bp`
    /// which is the percentage of the `paid` in basis points.
    struct Config has store, drop { amount_bp: u16 }

    /// When a Rule is added, configuration details are specified
    public fun set<T>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        amount_bp: u16
    ) {
        assert!(amount_bp <= MAX_BP, 0);
        policy::add_rule(Rule {}, policy, cap, Config { amount_bp })
    }

    /// To get the Receipt, the buyer must call this function and pay
    /// the required amount; the amount is calculated dynamically and
    /// it is more convenient to use a mutable reference
    public fun pay<T>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: &mut Coin<SUI>,
        ctx: &mut TxContext
    ) {
        // using the getter to read the paid amount
        let paid = policy::paid(request);
        let config: &Config = policy::get_rule(Rule {}, policy);
        let amount = (((paid as u128) * (config.amount_bp as u128) / MAX_BP) as u64);
        assert!(coin::value(payment) >= amount, EInsufficientAmount);

        let fee = coin::split(payment, amount, ctx);
        policy::add_to_balance(Rule {}, policy, fee);
        policy::add_receipt(Rule {}, request)
    }
}
```

## Time is also Money

Rules don't need to be only for payments and fees. Some might allow trading before or after a certain time. Since Rules are not standardized and can use anything, developers can encode logic around using any objects.

```move
module examples::time_rule {
    // skipping some dependencies
    use sui::clock::{Self, Clock};

    struct Rule has drop {}
    struct Config has store, drop { start_time: u64 }

    /// Start time is yet to come
    const ETooSoon: u64 = 0;

    /// Add a Rule that enables purchases after a certain time
    public fun set<T>(/* skip default fields */, start_time: u64) {
        policy::add_rule(Rule {}, policy, cap, Config { start_time })
    }

    /// Pass in the Clock and prove that current time value is higher
    /// than the `start_time`
    public fun confirm_time<T>(
        policy: &TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        clock: &Clock
    ) {
        let config: &Config = policy::get_rule(Rule {}, policy)
        assert!(clock::timestamp_ms(clock) >= config.start_time, ETooSoon);
        policy::add_receipt(Rule {}, request)
    }
}
```

## Generalizing approach: Witness policy

Sui Move has two main ways for authorizing an action: static - by using the Witness pattern, and dynamic - via the Capability pattern. With a small addition of type parameters to the Rule, it is possible to create a *generic Rule* which will not only vary by configuration but also by the type of the Rule.

```move
module examples::witness_rule {
    // skipping dependencies

    /// Rule is either not set or the Witness does not match the expectation
    const ERuleNotSet: u64 = 0;

    /// This Rule requires a witness of type W, see the implementation
    struct Rule<phantom W> has drop {}
    struct Config has store, drop {}

    /// No special arguments are required to set this Rule, but the
    /// publisher now needs to specify a Witness type
    public fun set<T, W>(/* .... */) {
        policy::add_rule(Rule<W> {}, policy, cap, Config {})
    }

    /// To confirm the action, buyer needs to pass in a witness
    /// which should be acquired either by calling some function or
    /// integrated into a more specific hook of a marketplace /
    /// trading module
    public fun confirm<T, W>(
        _: W,
        policy: &TransferPolicy<T>,
        request: &mut TransferRequest<T>
    ) {
        assert!(policy::has_rule<T, Rule<W>>(policy), ERuleNotSet);
        policy::add_receipt(Rule<W> {}, request)
    }
}
```

The "witness_rule" is very generic and can be used to require a custom Witness depending on the settings. It is a simple and yet a powerful way to link a custom marketplace / trading logic to the TransferPolicy. With a slight modification, the rule can be turned into a generic Capability requirement (basically any object, even a TransferPolicy for a different type or a TransferRequest - no limit to what could be done).

```move
module examples::capability_rule {
    // skipping dependencies

    /// Changing the type parameter name for better readability
    struct Rule<phantom Cap> has drop {}
    struct Config {}

    /// Absolutely identical to the witness setting
    public fun set<T, Cap>(/* ... *) {
        policy::add_rule(Rule<Cap> {}, policy, cap, Config {})
    }

    /// Almost the same with the Witness requirement, only now we
    /// require a reference to the type.
    public fun confirm<T, Cap>(
        cap: &Cap,
        /* ... */
    ) {
        assert!(policy::has_rule<T, Rule<Cap>>(policy), ERuleNotSet);
        policy::add_receipt(Rule<Cap> {}, request)
    }
}
```
