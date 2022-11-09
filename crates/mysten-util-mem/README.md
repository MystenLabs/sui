# mysten-util-mem

This crate provides tools for measuring the heap memory usage of specific structures.

## Annotating types with `MallocSizeOf` trait

To measure your struct's memory usage, it and all of its child types must implement the `MallocSizeOf` trait.

For types that are local to your crate, this is really easy. Just add:

```rust
#[derive(MallocSizeOf)]
```

For external types, you'll need to implement the trait here in [`external_impls.rs`](https://github.com/MystenLabs/blob/main/crates/mysten-util-mem/src/external_impls.rs). See the existing implementations in that file for examples.

Note that `size_of` should return only the number of **heap-allocated bytes** used by the type. For example, a type such as `struct MyStruct([u8; 128])` would return **zero**, not 128. Recursive accounting for heap-allocated memory when your struct is part of e.g. a `Vec` or `HashMap` is already taken care of by the implementations of `MallocSizeOf` on those collection types.

Oftentimes, the public interface of the type you are measuring does not provide enough information to precisely measure the amount of heap space it allocates. In that case, you can try just to produce a reasonable estimate.

## Measuring memory usage

To compute the heap usage of an annotated type at runtime, simply call `mysten_util_mem::malloc_size(&my_struct)`. For complete memory usage, add in the inline size of the type as well, as in:

```rust
mysten_util_mem::malloc_size(&my_struct) + std::mem::size_of::<MyStruct>()
```

## Putting it all together

For an example PR using library, take a look at https://github.com/MystenLabs/narwhal/pull/898/files.
