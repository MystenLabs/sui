# One Time Witness

One Time Witness (OTW) is a special instance of a type which is created only in the module initializer and is guaranteed to be unique and have only one instance. It is important for cases where we need to make sure that a witness-authorized action was performed only once (for example - [creating a new Coin](/samples/coin.md)). In Sui Move a type is considered an OTW if its definition has the following properties:

- Named after the module but uppercased
- Has only `drop` ability

To check whether an instance is an OTW, [`sui::types::is_one_time_witness(witness)`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/types.move) should be used.

To get an instance of this type, you need to add it as the first argument to the `init()` function: Sui runtime supplies both initializer arguments automatically.

```move
module examples::mycoin {

    /// Name matches the module name
    struct MYCOIN has drop {}

    /// The instance is received as the first argument
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        /* ... */
    }
}
```

---

Example which illustrates how OTW could be used:

```move
{{#include ../../examples/sources/basics/one-time-witness.move:4:}}
```
