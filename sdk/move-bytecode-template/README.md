# Move Bytecode Template

Move Bytecode Template allows updating a pre-compiled bytecode, so that a standard template could be customized and used to publish new modules on Sui directly in the browser. Hence, removing the need for a backend to compile new modules.

This crate builds a WASM binary for the `move-language/move-binary-format` allowing bytecode serialization and deserialization in various environments. The main target for this package is "web".

## Applications

This package is a perfect fit for the following applications:

- Publishing new Coins
- Publishing TransferPolicies
- Initializing any base type with a custom sub-type

## Example of a Template Module

The following code is a close-copy of the `Coin` example in the [Move by Example](https://examples.sui.io/samples/coin.html).

```move
module 0x0::template {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// The OTW for the Coin
    struct TEMPLATE has drop {}

    const DECIMALS: u8 = 0;
    const SYMBOL: vector<u8> = b"TMPL";
    const NAME: vector<u8> = b"Template Coin";
    const DESCRIPTION: vector<u8> = b"Template Coin Description";

    /// Init the Coin
    fun init(witness: TEMPLATE, ctx: &mut TxContext) {
        let (treasury, metadata) = coin::create_currency(
            witness, DECIMALS, SYMBOL, NAME, DESCRIPTION, option::none(), ctx
        );

        transfer::public_transfer(treasury, tx_context::sender(ctx));
        transfer::public_transfer(metadata, tx_context::sender(ctx));
    }
}
```

## Usage in Web applications

The package consists of code and a wasm binary. While the former can be imported directly, the latter should be made available in static / public assets as a Web application. Initialization needs to be performed via a URL, and once completed, other functions become available.

```ts
import init, initSync, * as wasm from '@mysten/move-bytecode-template';

await init('path/to/move_binary_format_bg.wasm');
// alternatively initSync(...);

let version = wasm.version();
let json = wasm.deserialize('a11ceb0b06....');
let bytes = wasm.serialize(json);

console.assert(json == bytes, '(de)serialization failed!');
```

## Using with Vite

To use this package with Vite, you need to import the source file and the wasm binary.

```ts
import init, * as bytecodeTemplate from "@mysten/move-bytecode-template";
import url from '@mysten/move-bytecode-template/move_bytecode_template_bg.wasm?url';
```

Later, you can initialize the package with the URL.

```ts
await init(url);
```

Lastly, once the package is initialized, you can use the functions as described in the previous section.

```ts
const template = "a11ceb0b06....";

bytecodeTemplate.deserialize(template);
bytecodeTemplate.version();
bytecodeTemplate.update_identifiers(template, {
    "TEMPLATE": "MY_MODULE",
    "template": "my_module"
});
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
