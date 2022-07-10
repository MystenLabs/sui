# Init Function

Init function is a special function which gets executed only once - when module is published. It always has the same signature and only
one argument.
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

Example:

```move
{{#include ../../examples/sources/basics/init-function.move:4:}}
```
