# Move Binary Format

This crate builds a WASM binary for the `move-language/move-binary-format` allowing bytecode serialization and deserialization in various environments. The main environment this package targets is "web".

## Usage in Web applications

The package consists of code and a wasm binary. While the former can be imported directly, the latter should be made available in static / public assets as a Web application. Initialization needs to be performed via a URL, and once completed, other functions become available.

```ts
import init, initSync, * as wasm from '@mysten/move-binary-format';

await init('...path to /move_binary_format_bg.wasm');
// alternatively initSync(...);

let version = wasm.version();
let json = wasm.deserialize('a11ceb0b06....');
let bytes = wasm.serialize(json);

console.assert(json == bytes, '(de)serialization failed!');
```

## Build locally

To build the binary, you need to have Rust installed and then the `wasm-pack`. The installation script [can be found here](https://rustwasm.github.io/wasm-pack/).

Building for test (nodejs) environment - required for tests.
```
pnpm build:dev
```

Building for web environment.
```
pnpm build:release
```

## Running tests

Local tests can only be run on the `dev` build. To run tests, follow these steps:

```
pnpm build:dev
pnpm test
```
