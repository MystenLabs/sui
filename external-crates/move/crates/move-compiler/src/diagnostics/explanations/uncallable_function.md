`TxContext`, `Clock`, and `Random` are values supplied by the Sui transaction runtime. A caller
cannot construct arbitrary instances of them, so a function is transaction-callable only when its
parameters match the forms the runtime can provide.

This lint reports any of these signatures:

- `TxContext` by value. It must be borrowed as `&TxContext` or `&mut TxContext`.
- A `&mut TxContext` together with any other `TxContext` parameter. The mutable borrow must be the
  function's only use of `TxContext`; multiple immutable `&TxContext` parameters are allowed.
- `Clock` or `Random` by value or by mutable reference. These shared system objects are available
  only as `&Clock` and `&Random`.

## Example

Flagged:

```move
fun owned_context(_ctx: TxContext) {}

fun duplicate_context(_ctx: &mut TxContext, _again: &TxContext) {}

fun mutable_system_objects(_clock: &mut Clock, _random: Random) {}
```

Suggested:

```move
fun reads_system_state(_clock: &Clock, _random: &Random, _ctx: &TxContext) {}

fun updates_transaction(_ctx: &mut TxContext) {}
```
