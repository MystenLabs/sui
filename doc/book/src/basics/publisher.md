# Publisher

Publisher Object serves as a way to represent the publisher authority. The object itself does not imply any specific use case and has only two main functions: `package::from_module<T>` and `package::from_package<T>` which allow checking whether a type `T` belongs to a module or a package for which the `Publisher` object was created.

We strongly advise to issue the `Publisher` object for most of the packages that define new Objects - it is required to set the "Display" as well as to allow the type to be traded in the "Kiosk" ecosystem.

> Although `Publisher` itself is a utility, it enables the _"proof of ownership"_ functionality, for example, it is crucial for [the Object Display](./display.md).

To set up a Publisher, a One-Time-Witness (OTW) is required - this way we ensure the `Publisher` object is initialized only once for a specific module (but can be multiple for a package) as well as that the creation function is called in the publish transaction.

```move
{{#include ../../examples/sources/basics/publisher.move:4:}}
```
