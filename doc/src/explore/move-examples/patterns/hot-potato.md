# Hot Potato

Hot Potato is a name for a struct that has no abilities, hence it can only be packed and unpacked in its module. In this struct, you must call function B after function A in the case where function A returns a potato and function B consumes it.

```move
{{#include ../../examples/sources/patterns/hot_potato.move:4:}}
```

This pattern is used in these examples:

- [Flash Loan](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/flash_lender.move)
