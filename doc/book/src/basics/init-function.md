# Init Function

Init function is a special function that gets executed only once - when the associated module is published. It always has the same signature and only
one argument:
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

For example:

```move
{{#include ../../examples/sources/basics/init-function.move:4:}}
```
