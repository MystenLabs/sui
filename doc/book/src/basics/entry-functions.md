# Entry Functions

An [entry function](https://docs.sui.io/build/move#entry-functions) visibility modifier allows a function to be called directly (eg in transaction). It is combinable with other
visibility modifiers, such as `public` which allows calling from other modules) and `public(friend)` for calling from *friend* modules.

```move
{{#include ../../examples/sources/basics/entry-functions.move:4:}}
```
