# Witness

Witness is a pattern that is used for confirming the ownership of a type. To do so, one passes a `drop` instance of a type. Coin relies on this implementation.

```move
{{#include ../../examples/sources/patterns/witness.move:4:}}
```
