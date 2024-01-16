# Transfer-to-Object: Cash Register Example

This document explores various methods of implementing a cash register that can
accept and process payments on Sui. We'll focus on highlighting the trade-offs
of each approach. Through these examples, you'll gain insights into the new
transfer-to-object functionality and understand some of its applications and
the types of issues it can address.

## Representing Payments

Before getting started, we need to define a common method for making payments.
Each payment is an object that consists of a `payment_id`, which is a unique
identifier for the payment (i.e., a way of tracking what the payment was for),
along with the actual `coin` for the payment.

```move
/// A unique payment for a good or service that can be uniquely identified by
/// `payment_id`.
struct IdentifiedPayment has key, store {
    /// Object ID
    id: UID,
    /// The unique id for the good/service being paid for
    payment_id: u64,
    /// The payment
    coin: Coin<SUI>,
}
```

Using this, customers can make a payment with a unique payment ID to an address
using the function `fun make_payment(payment_id: u64, coin: Coin<SUI>, to: address)`.
This function creates an `IdentifiedPayment`, sends it to the `to`
address, and emits an event with the payment's ID, the recipient, the amount
paid, and the payer.

Once the receiver of the payment has the `IdentifiedPayment` object, they can
`unpack` the identified payment into the coin that was sent. This will then
emit a separate event that marks the payment ID within the `IdentifiedPayment`
as processed.

You can see the Move code for this section (along with `EarmarkedPayment`s,
which we'll use later on) [here](./common/sources/identified_payment.move).

Now that we have established how we'll represent payments, let's examine a
couple of different ways you could represent a cash register or perform
customer-to-business type transactions on-chain.

## Implementation 1: Using an Account Address

Imagine you run a restaurant. Your business will have an
address `A` on-chain. When an order is taken, the customer will simply make a
payment by transferring an `IdentifiedPayment` object with the payment ID you provided
to them to your address `A`.

Whenever an `IdentifiedPayment` is sent, you'll be able to track it and mark
the bill as paid when you see the `SentPaymentEvent` with the given payment ID
that you provided and match it against the amount owed.

Later on (either asynchronously or in a batch at the end of the day), you can
process the payments you've received by iterating over the set of
`IdentifiedPayment` objects under your account, `unpack`ing them, and then
using the unpacked SUI coin.

Overall, this is a very simple representation for on-chain payments and
relatively easy to set up. However, it has some issues:

1. If your private key(s) for `A` are compromised, you would need to change to
   a different address. This could cause issues for customers still using the
   older address for the business.
2. If you want to allow multiple employees to access the cash register, it can
   only be done via a multi-sig policy. However, this could present issues if
   an employee departs or is compromised, or if there are a large number of
   employees that you want to allow access to payments.

You can see the Move implementation for this section [here](./owned-no-tto/sources/cash_register.move).

## Implementation 2: Using a Shared Object

To address some of the issues mentioned above, you could instead have your restaurant
use a shared "cash register" object for payments and have customers pay into
this shared object. In particular:

1. If the private key(s) of the register owner are compromised, a new address
   can be created and the `owner` field of the shared `Register` object can be
   set to the new address.
2. Additional employees can be added to the `Register`'s `authorized_employees`
   list. If an employee departs or is hired, they can easily be removed from or
   added to this list without changing the object ID of the shared `Register`
   object that customers interact with.

However, with the shared `Register`, payments must be made differently than
simply transferring the coins to the shared object. In
particular, without transfer-to-object, a payment to the `Register` object
would involve taking the shared `Register` object for the restaurant and adding
the payment as a dynamic object field under it:

```move
public fun make_shared_payment(
    register_uid: &mut UID,
    payment_id: u64,
    coin: Coin<SUI>,
    ctx: &mut TxContext
) {
    let identified_payment = IdentifiedPayment {
        id: object::new(ctx),
        payment_id,
        coin,
    };
    // Add the payment as a dynamic field under the register object
    dynamic_field::add(register_uid, payment_id, identified_payment)
}
```

Because of this, if your restaurant becomes incredibly popular across multiple
locations and you need to serve hundreds or thousands of customers at once,
those customers' payments must all be processed serially since they would all
be using the same shared object. This could lead to contention over the
`Register` object and delays in payment processing. In contrast, with
Implementation 1, since it uses only owned objects, all payments across all of
your restaurant locations could be processed in parallel and not effect each
other.

Luckily, transfer-to-object can help parallelize the payment process to the
`Register` object while also keeping the benefits of dynamic authorization and
stable interaction IDs that we saw in this implementation. Let's examine
exactly how it does this in the next example.

You can see the Move implementation for this section [here](./shared-no-tto/sources/shared_cash_register.move).

## Implementation 3: Using a Shared Object + Transfer-to-Object

With transfer-to-object, we can combine the benefits of the two previous implementations:

- The object ID stability of the shared object register.
- The ability to set a different owner of the `Register` object in case of key
  compromise (or e.g., selling the business) by changing the `owner` field.
- An easy way of dynamically adding, removing, and enforcing permissions on who
  can withdraw payments.
- Payments can still be made using the `identified_payment::make_payment`
  function that uses `sui::transfer::transfer` under the hood, so payments can
  happen in parallel across all restaurant locations without needing to be
  sequenced against the shared `Register` object.

You can see the entire implementation for the shared object register using
transfer-to-object [here](./shared-with-tto/sources/shared_cash_register.move).

Let's go through this implementation in more detail and compare it to the above
two implementations.

### Interaction Stability: Object ID Remains the Same

To make a payment, nothing changes from Implementation 1. In particular,
customers will still use `identified_payment::make_payment` and simply set the
address they want to send to be the object ID of the restaurant's `Register`
object. If the restaurant changes the ownership of the `Register` object, this
will be opaque to the customers – they will always send their payment to the
same `Register` object.

### Receiving Payments

At a high level, handling payments after they have been made using
transfer-to-object resides somewhere between both Implementation 1 and
Implementation 2. In particular:

- Similar to Implementation 1, the object IDs of the payments you want to
  handle in that transaction will show up in the transaction's inputs.
- Similar to Implementation 2, there are dynamic checks enforced on being able
  to access the sent payments.

To understand what's going on here, it's best to go through the implementation
of `handle_payment`:

```move
/// We take the `Register` shared object mutably, along with a "ticket"
// `handle_payment` that we can exchange for the actual `IdentifiedPayment` object
// that it is associated with.
public fun handle_payment(
    register: &mut Register,
    handle_payment: Receiving<IdentifiedPayment>,
    ctx: &TxContext
): IdentifiedPayment {
    // If the sender of the transaction that wants to handle this payment is in
    // the list of authorized employees in the `Register` object then we will
    // permit them to withdraw the `IdentifiedPayment` object.
    assert!(
        vector::contains(&register.authorized_employees, tx_context::sender(ctx)),
        ENotAuthorized
    );
    // Authorization check successful -- exchange the `handle_payment` ticket
    // for the `IdentifiedPayment` object and return it.
    transfer::public_receive(&mut register.id, handle_payment)
}
```

### Adding Tips Using a Custom `Receive` Rule

One additional benefit of transfer-to-object is that, in addition to being able
to specify custom transfer rules for `key`-only objects, you can also specify
custom receiving rules for `key`-only objects in a very similar manner: if an
object is `key`-only, then the `sui::transfer::receive` function can be called
in the module that defines the object, but not elsewhere. Elsewhere, the
`sui::transfer::public_receive` function must be called and can only be used on
objects that also have the `store` ability.

With this information, we can define a wrapper around `IdentifiedPayment`s
where we can earmark that payment for a specific address, e.g., the address of
our server at the restaurant. We can then use the custom receive rule to ensure
that only our server can access their tip and no one else can.

```move
struct EarmarkedPayment has key {
    id: UID,
    payment: IdentifiedPayment,
    for: address,
}
```

Since `EarmarkedPayment` is `key` only, we can then define a custom receiving
rule for it so that only the address that we specified for it can receive the
payment:

```move
public fun receive(
    parent: &mut UID,
    ticket: Receiving<EarmarkedPayment>,
    ctx: &TxContext
): IdentifiedPayment {
    let EarmarkedPayment { id, payment, for } = transfer::receive(parent, ticket);
    // If the sender isn't the address we specified, the transaction will abort.
    assert!(tx_context::sender(ctx) == for, ENotEarmarkedForSender);
    object::delete(id);
    payment
}
```

You can see the implementations for `EarmarkedPayment`s, the custom receiving
rules, and functions at the bottom of the file
[here](./common/sources/identified_payment.move).
