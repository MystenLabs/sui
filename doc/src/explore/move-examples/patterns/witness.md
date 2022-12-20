# Witness

Witness is a pattern that is used for confirming the ownership of a type. To do so, one passes a `drop` instance of a type. Coin relies on this implementation.

```move
{{#include ../../examples/sources/patterns/witness.move:4:}}
```

This pattern is used in these examples:

- [Liquidity pool](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/pool.move)
- [Regulated coin](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/fungible_tokens/sources/regulated_coin.move)
