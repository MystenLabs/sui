# Hot Potato

Hot Potato is a name for a struct that has no abilities, hence it can only packed and unpacked in its module. The pattern itself creates a case where function B has to be called after function A if A returns a potato and B consumes it.

```move
{{#include ../../examples/sources/patterns/hot-potato.move:4:}}
```
