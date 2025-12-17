# Changes to Non-`public` `entry` Functions in PTBs

In the next release (v1.62), there will be a new set of verification rules for arguments to non-`public` (either private or `public(package)`) `entry` functions. These rules will fully replace the existing rules, and in most cases will allow for more expressivity! This means that you can do more with `entry` functions than previously possible.

While we have received the feedback that most people do not understand the existing rules around `entry` functions, this post will not explain them since they are going away. Instead we'll focus on the new `entry` function rules going forward.

## Overview

For a brief overview, arguments to a non-`public` `entry` function cannot be entangled with a hot potato.
For example with the following code

```move
module ex::m;

public struct HotPotato()

public fun hot<T>(x: &mut Coin<T>): HotPotato { ... }
entry fun spend<T>(x: &mut Coin<T>) { ... }
public fun cool(h: HotPotato) { ... }
```

With an example PTB, this is invalid since the input coin to `spend` has an entangled hot potato when it is used with the `spend` function

```move
// Invalid PTB
0: ex::m::hot(Input(0));
1: ex::m::spend(Input(0)); // INVALID, Input(0) still hot via Result(0)
2: ex::m::cool(Result(0));
```

However, it is valid if the hot potato is destroyed before `spend` is called.

```move
// Valid PTB
0: ex::m::hot(Input(0));
1: ex::m::cool(Result(0));
2: ex::m::spend(Input(0)); // Valid! Input(0) is not hot
```

Below we will dig deeper into why these rules exist and how the rules are defined.

## The New Rules

### Motivation

You might wonder why have any rules for the usage of values with `entry` functions? The original motivation was to ensure that package developers had a way of ensuring a certain sense of “atomicity” for the arguments to their `entry` functions. Meaning a way of ensuring that the arguments would behave the same if the specific `entry` function was the only command in the PTB. A canonical example for this is flash loans—a developer might want to ensure that a given `Coin` is not from a flash loan and is ostensibly “owned” by the sender of the transaction.

In Move, flash loans (and similar paradigms) use [“hot potato”](https://move-book.com/programmability/hot-potato-pattern) patterns to force behavior.
For example

```move
module flash::loan;

use sui::balance::Balance;
use sui::sui::SUI;

public struct Bank has key {
    id: UID,
    holdings: Balance<SUI>,
}

// This is a hot potato because it does not have `store` and does not have `drop`
public struct Loan {
    amount: u64,
}

public fun issue(bank: &mut Bank, amount: u64): (Balance<SUI>, Loan) {
    assert!(bank.holdings.value() >= amount);
    let loaned = bank.holdings.split(amount);
    (loaned, Loan { amount })
}

public fun repay(bank: &mut Bank, loan: Loan, repayment: Balance<SUI>) {
    let Loan { amount } = loan;
    assert!(repayment.value() == amount);
    bank.holdings.join(repayment);
}

```

In this example, when `issue` is called, a `Loan` hot potato is created. In the PTB if `issue` is called, the transaction will not succeed unless the created `Loan` hot potato is destroyed by calling `repay`.

Our goal with non-public `entry` functions is to ensure that no argument is involved in such a flash loan (or similar hot potato) scenario. In other words, the arguments to a non-public `entry` function cannot be entangled in such a way to forces behavior in the PTB after the `entry` function is called. We will track this with an algorithm that tries to count how many hot potato values are active and what values they can influence.

### Terminology

Some brief terminology before looking at the rules and their defining algorithm.

- The rules apply to the PTB _statically_. This means that the verification happens before the PTB begins execution. In some cases (particularly around shared objects), this will result in the rules seeming more general and pessimistic than they otherwise would be if they were applied _dynamically_ as the PTB was executed.
- A _value_ is any PTB `Argument`. These can be `Input`s, `Result`s, `NestedResult`s, or the `GasCoin` (already smashed).
- A _result_ is a value that was returned from a PTB command. These are referred to via `Result` and `NestedResult`.
- Arguments to a PTB command have two usage types: by-reference (`&` or `&mut`) or by-value (either copied or moved).
- A value is considered _hot_ if its type has neither `store` nor `drop`.
- This means a hot value’s type can be in one of the following cases:
  - No abilities
  - `copy`
  - `key`
  - Note that a value cannot have both `key` and `copy` since `sui::object::UID` does not have `copy`
- Each value belongs to a _clique_. A clique represents values that have been used together as arguments and their results.
- Each clique has a count with the number of hot values. Meaning that the clique’s count is incremented when results are hot (once per result), and the clique’s count is decremented when a hot value is moved (taken by-value and not copied).
  - The count here is tracking how many hot potato (or similar) values are outstanding, and the clique is tracking which values they could restrict or otherwise influence.

### The Algorithm

- Each input to the PTB starts off in its own clique with a count of zero.
- When values are used (by reference or by-value) together in a command, their cliques are merged, adding together each clique’s count.
- The count of the arguments’ merged clique is decremented for each hot value moved (taken by-value and not copied).
- If the command is a Move call for a non-public `entry` function, the count of the arguments’ merged clique must be zero at this point.
  - Note that this means a non-public `entry` function _can_ take hot values! They must just be the last hot values in their clique.
- Results of each command are included in the arguments’ merged clique. The clique’s count is incremented for each hot result value.
- **NOTE:** Shared objects taken by-value have a special rule in that during the accounting for the result values, the argument’s merged clique’s count is set to infinity.
  - See the “Limitations” section below for more detail

### Examples

Walking through the example from the overview more carefully with the algorithm.
In these examples, we will walk through the algorithm, showing each clique and its count between each command.

```move
// Invalid PTB
// Input 0: Coin<SUI>
// cliques: { Input(0) } => 0
0: ex::m::hot(Input(0));
// cliques: { Input(0), Result(0) } = 1
1: ex::m::spend(Input(0)); // INVALID, Input(0)'s clique has a count > 0
2: ex::m::cool(Result(0));

// Valid PTB
// Input 0: Coin<SUI>
// cliques: { Input(0) } => 0
0: ex::m::hot(Input(0));
// cliques: { Input(0), Result(0) } = 1
1: ex::m::cool(Result(0));
// cliques: { Input(0) } => 0
2: ex::m::spend(Input(0)); // Valid! Input(0)'s clique has a count of 0
```

Using the `flash::loan` module from above, we can construct more involved examples

```move
// Invalid PTB
// Input 0: flash::loan::Bank
// Input 1: u64
// cliques: { Input(0) } => 0, { Input(1) } =>  0,
0: flash::loan::issue(Input(0), Input(1))
// cliques: { Input(0), NestedResult(0,0), NestedResult(0,1) }  =>  1,
1: sui::coin::from_balance(NestedResult(0,0));
// cliques: { Input(0), NestedResult(0,1), Result(1) }  =>  1,
2: ex::m::spend(Result(1)); // INVALID, Result(1)'s clique has count > 0
3: sui::coin::into_balance(Result(1));
4: flash::loan::repay(Result(3), NestedResult(0,1));
```

Even though the `Coin` created in command `1` was not directly involved in the flash loan in command `0`, its a part of a clique with a hot value `NestedResult(0,1)`. As such, it cannot be used in the private `entry` function `ex::m::spend`.

If the loan was repaid with `flash::loan::repay` before `ex::m::spend` was called, then this would be permitted (like we saw with the earlier example).

## Limitations

As mentioned above, a clique with a shared object by-value is always hot. In other words, a non-public `entry` function can take a shared object by-value, but it cannot take a value in a clique that previously interacted with a shared object by value.

Why? This rule is needed since shared objects cannot be wrapped—they either have to be re-shared or deleted. This means that a shared-object could be used to force behavior in a way similar to a hot potato. But unlike a hot potato, we cannot tell from signature of the function if it is used properly.

If this algorithm was “dynamic” rather than “static”, it could be more precise at the cost of clarity. That is, a static set of rules is typically easier to describe and follow as compared to a dynamic set of rules. However, [party objects](https://docs.sui.io/guides/developer/objects/object-ownership/party) will fall under this restriction under more narrow cases than with shared objects. As such, we think that this restriction will be acceptable long term without having to sacrifice the clarity of the static system.

## Coming Soon (v1.63 or later)

In a later version, we will remove the signature restrictions for `entry` functions. This means that _any_ Move function can become `entry`!
