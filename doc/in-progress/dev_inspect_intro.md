# `dev-inspect`: Test any Move function with any arguments

## What is it?

For a normal transaction, a Move `entry` function cannot return values. Instead, objects are used for state changes, and events are used for signaling.
However, not having return values can be frustrating if you just want to perform some computation using on-chain data. And with the introduction of dynamic fields, that computation could be even more difficult to perform without using the real, live data.

`dev-inspect` attempts to alleviate these pain points and more, by letting a user call any Move function with any arguments. Return values and changes to mutable inputs will be returned to the user. And inputs can be provided through either object IDs or with the BCS bytes of the value, for any value! And no state changes will be made (and no gas will be charged). This entry point should be helpful for inspecting on chain data, or for testing specific Move functions.

## Using `dev-inspect`

To use `dev-inspect` there are two modes of invocation, via a normal transaction payload or a direct Move call. Any object (owned, shared, or immutable) can be used in the call, and does not need to be owned by the sender and the usage of the shared object does not need to go through consensus.

NEED EXAMPLE FOR NORMAL TXN CALL

For the direct Move call, you just need to specify the function to call and the arguments. The runtime will provide a fake gas coin to use.

EXAMPLE

In either case, you can always specify the exact BCS bytes for any value (not just primitives), which is not normally allowed with `entry` functions and normal transactions

EXAMPLE

This also means that object arguments can be populated either by object ID or by their BCS bytes! This can be useful for testing scenarios, even when you don't have an example object in storage to use.

## Future Work

Currently, the response from `dev-inspect` does not include the data of objects after execution. The only way to view results from either the return values, or from the modified inputs. In other words, the response will include the values returned from the Move function invoked, and the final value of arguments passed in via a mutable reference, `&mut`. It will also include what objects were created, mutated, or deleted, but it will not include the data for those objects (just the digest). The full data will be added in the future.

Also, the epoch value is currently provided by the fullnode. This will likely move to being an argument to the `dev-inspect` call.

Please let us know if you have any feedback on the feature, or if there is anything we could do to make it easier to use!
