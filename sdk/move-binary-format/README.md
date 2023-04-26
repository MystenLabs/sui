# Move Binary Format WASM

This crate builds a WASM binary for the `move-language/move-binary-format` allowing bytecode serialization and deserialization in various settings.

## Prerequisites

To build the binary, you need to install the `wasm-pack`. The installation script [can be found here](https://rustwasm.github.io/wasm-pack/).

## Compiling

Building the binary for Web.

```
wasm-pack build --target web
```
