# Custom transfer

In Sui Move, objects defined with only `key` ability can not be transferred by default. To enable
transfers, publisher has to create a custom transfer function. This function can include any arguments,
for example a fee, that users have to pay to transfer.

```move
{{#include ../../examples/sources/basics/custom-transfer.move:4:}}
```
