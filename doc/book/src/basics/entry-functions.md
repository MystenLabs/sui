# Entry Functions

Entry function visibility modifier allows a function to be called directly (eg in transaction). It is combinable with other
visibility modifiers such as `public` (allows calling from other modules), `public(friend)` - for calling from *friend* modules.

```move
{{#include ../../examples/sources/basics/entry-functions.move:4:}}
```
