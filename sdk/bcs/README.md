# BCS - Binary Canonical Serialization

This small and lightweight library implements [Binary Canonical Serialization (BCS)](https://github.com/zefchain/bcs) in JavaScript, making BCS available in both Browser and NodeJS environments.

## Install

To install, add the [`@mysten/bcs`](https://www.npmjs.com/package/@mysten/bcs) package to your project:

```sh
npm i @mysten/bcs
```

## Quickstart

```ts
import { BCS, getSuiMoveConfig } from "@mysten/bcs";

const bcs = new BCS(getSuiMoveConfig());

// registering types so we can use them
bcs.registerAlias("UID", BCS.ADDRESS);
bcs.registerEnumType("Option<T>", {
  none: null,
  some: "T",
});
bcs.registerStructType("Coin", {
  id: "UID",
  value: BCS.U64,
});

// deserialization: BCS bytes into Coin
let bytes = bcs
  .ser("Coin", {
    id: "0000000000000000000000000000000000000000000000000000000000000001",
    value: 1000000n,
  })
  .toBytes();
let coin = bcs.de("Coin", bcsBytes, "hex");

// serialization: Object into bytes - an Option with <T = Coin>
let data = bcs.ser("Option<Coin>", { some: coin }).toString("hex");

console.log(data);
```

## Description

BCS defines the way the data is serialized, and the result contains no type information. To be able to serialize the data and later deserialize, a schema has to be created (based on the built-in primitives, such as `address` or `u64`). There's no tip in the serialized bytes on what they mean, so the receiving part needs to know how to treat it.

## Configuration

BCS constructor is configurable for the target. The following parameters are available for custom configuration:

| parameter           | required | description                                                               |
| ------------------- | -------- | ------------------------------------------------------------------------- |
| `vectorType`        | +        | Defines the type of the vector (`vector<T>` in SuiMove, `Vec<T>` in Rust) |
| `addressLength`     | +        | Length of the built-in `address` type. 20 for SuiMove, 32 for Core Move   |
| `addressEncoding`   | -        | Custom encoding for addresses - "hex" or "base64"                         |
| `genericSeparators` | -        | Generic type parameters syntax, default is `['<', '>']`                   |
| `types`             | -        | Define enums, structs and aliases at initialization stage                 |
| `withPrimitives`    | -        | Whether to register primitive types (`true` by default)                   |

```ts
// Example: All options used
import { BCS } from "@mysten/bcs";

const SUI_ADDRESS_LENGTH = 32;
const bcs = new BCS({
  vectorType: "vector<T>",
  addressLength: SUI_ADDRESS_LENGTH,
  addressEncoding: "hex",
  genericSeparators: ["<", ">"],
  types: {
    // define schema in the initializer
    structs: {
      User: {
        name: BCS.STRING,
        age: BCS.U8,
      },
    },
    enums: {},
    aliases: { hex: BCS.HEX },
  },
  withPrimitives: true,
});

let bytes = bcs.ser("User", { name: "Adam", age: "30" }).toString("base64");

console.log(bytes);
```

For Sui Move there's already a pre-built configuration which can be used through the `getSuiMoveConfig()` call.

```ts
// Example: Sui Move Config
import { BCS, getSuiMoveConfig } from "@mysten/bcs";

const bcs = new BCS(getSuiMoveConfig());

// use bcs.ser() to serialize data
const val = [1, 2, 3, 4];
const ser = bcs.ser(["vector", BCS.U8], val).toBytes();

// use bcs.de() to deserialize data
const res = bcs.de(["vector", BCS.U8], ser);

console.assert(res.toString() === val.toString());
```

Similar configuration exists for Rust, the difference is the `Vec<T>` for vectors and `address` (being a special Move type) is not needed:

```ts
// Example: Rust Config
import { BCS, getRustConfig } from "@mysten/bcs";

const bcs = new BCS(getRustConfig());
const val = [1, 2, 3, 4];
const ser = bcs.ser(["Vec", BCS.U8], val).toBytes();
const res = bcs.de(["Vec", BCS.U8], ser);

console.assert(res.toString() === val.toString());
```

## Built-in types

By default, BCS will have a set of built-in type definitions and handy abstractions; all of them are supported in Move.

Supported integer types are: u8, u16, u32, u64, u128 and u256. Constants `BCS.U8` to `BCS.U256` are provided by the library.

| Type                        | Constant                      | Description                                            |
| --------------------------- | ----------------------------- | ------------------------------------------------------ |
| 'bool'                      | `BCS.BOOL`                    | Boolean type (converts to `true` / `false`)            |
| 'u8'...'u256'               | `BCS.U8` ... `BCS.U256`       | Integer types                                          |
| 'address'                   | `BCS.ADDRESS`                 | Address type (also used for IDs in Sui Move)           |
| 'vector\<T\>' \| 'Vec\<T\>' | _Only custom use, requires T_ | Generic vector of any element                          |
| 'string'                    | `BCS.STRING`                  | `vector<u8>` that (de)serializes to/from ASCII string  |
| 'hex-string'                | `BCS.HEX`                     | `vector<u8>` that (de)serializes to/from HEX string    |
| 'base64-string'             | `BCS.BASE64`                  | `vector<u8>` that (de)serializes to/from Base64 string |
| 'base58-string'             | `BCS.BASE58`                  | `vector<u8>` that (de)serializes to/from Base58 string |

---

All of the type usage examples below can be used for `bcs.de(<type>, ...)` as well.

```ts
// Example: Primitive types
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// Integers
let _u8 = bcs.ser(BCS.U8, 100).toBytes();
let _u64 = bcs.ser(BCS.U64, 1000000n).toString("hex");
let _u128 = bcs.ser(BCS.U128, "100000010000001000000").toString("base64");

// Other types
let _bool = bcs.ser(BCS.BOOL, true).toString("hex");
let _addr = bcs
  .ser(BCS.ADDRESS, "0000000000000000000000000000000000000001")
  .toBytes();
let _str = bcs.ser(BCS.STRING, "this is an ascii string").toBytes();

// Vectors (vector<T>)
let _u8_vec = bcs.ser(["vector", BCS.U8], [1, 2, 3, 4, 5, 6, 7]).toBytes();
let _bool_vec = bcs.ser(["vector", BCS.BOOL], [true, true, false]).toBytes();
let _str_vec = bcs
  .ser("vector<bool>", ["string1", "string2", "string3"])
  .toBytes();

// Even vector of vector (...of vector) is an option
let _matrix = bcs
  .ser("vector<vector<u8>>", [
    [0, 0, 0],
    [1, 1, 1],
    [2, 2, 2],
  ])
  .toBytes();
```

## Ser/de and formatting

To serialize and deserialize data to and from BCS there are two methods: `bcs.ser()` and `bcs.de()`.

```ts
// Example: Ser/de and Encoding
import { BCS, getSuiMoveConfig, BcsWriter } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// bcs.ser() returns an instance of BcsWriter which can be converted to bytes or a string
let bcsWriter: BcsWriter = bcs.ser(BCS.STRING, "this is a string");

// writer.toBytes() returns a Uint8Array
let bytes: Uint8Array = bcsWriter.toBytes();

// custom encodings can be chosen when needed (just like Buffer)
let hex: string = bcsWriter.toString("hex");
let base64: string = bcsWriter.toString("base64");
let base58: string = bcsWriter.toString("base58");

// bcs.de() reads BCS data and returns the value
// by default it expects data to be `Uint8Array`
let str1 = bcs.de(BCS.STRING, bytes);

// alternatively, an encoding of input can be specified
let str2 = bcs.de(BCS.STRING, hex, "hex");
let str3 = bcs.de(BCS.STRING, base64, "base64");
let str4 = bcs.de(BCS.STRING, base58, "base58");

console.assert((str1 == str2) == (str3 == str4), "Result is the same");
```

## Registering new types

> Tip: all registering methods start with `bcs.register*` (eg `bcs.registerStructType`).

### Alias

Alias is a way to create custom name for a registered type. It is helpful for fine-tuning a predefined schema without making changes deep in the tree.

```ts
// Example: Alias
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

bcs.registerAlias("ObjectDigest", BCS.BASE58);

// ObjectDigest is now treated as base58 string
let _b58 = bcs.ser("ObjectDigest", "Ldp").toBytes();

// we can override already existing definition to make it a HEX string
bcs.registerAlias("ObjectDigest", BCS.HEX);

let _hex = bcs.ser("ObjectDigest", "C0FFEE").toBytes();
```

### Struct

Structs are the most common way of working with data; in BCS, a struct is simply a sequence of base types.

```ts
// Example: Struct
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// register a custom type (it becomes available for using)
bcs.registerStructType("Balance", {
  value: BCS.U64,
});

bcs.registerStructType("Coin", {
  id: BCS.ADDRESS,
  // reference another registered type
  balance: "Balance",
});

// value passed into ser function has to have the same
// structure as the definition
let _bytes = bcs
  .ser("Coin", {
    id: "0x0000000000000000000000000000000000000000000000000000000000000005",
    balance: {
      value: 100000000n,
    },
  })
  .toBytes();
```

## Using Generics

To define a generic struct or an enum, pass the type parameters. It can either be done as a part of a string or as an Array. See below:
```ts
// Example: Generics
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// Container -> the name of the type
// T -> type parameter which has to be passed in `ser()` or `de()` methods
// If you're not familiar with generics, treat them as type Templates
bcs.registerStructType(["Container", "T"], {
  contents: "T"
});

// When serializing, we have to pass the type to use for `T`
bcs.ser(["Container", BCS.U8], {
  contents: 100
}).toString("hex");

// Reusing the same Container type with different contents.
// Mind that generics need to be passed as Array after the main type.
bcs.ser(["Container", [ "vector", BCS.BOOL ]], {
  contents: [ true, false, true ]
}).toString("hex");

// Using multiple generics - you can use any string for convenience and
// readability. See how we also use array notation for a field definition.
bcs.registerStructType(["VecMap", "Key", "Val"], {
  keys: ["vector", "Key"],
  values: ["vector", "Val"]
});

// To serialize VecMap, we can use:
bcs.ser(["VecMap", BCS.STRING, BCS.STRING], {
  keys: [ "key1", "key2", "key3" ],
  values: [ "value1", "value2", "value3" ]
});
```

### Enum

In BCS enums are encoded in a special way - first byte marks the order and then the value. Enum is an object, only one property of which is used; if an invariant is empty, `null` should be used to mark it (see `Option<T>` below).

```ts
// Example: Enum
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

bcs.registerEnumType("Option<T>", {
  none: null,
  some: "T",
});

bcs.registerEnumType("TransactionType", {
  single: "vector<u8>",
  batch: "vector<vector<u8>>",
});

// any truthy value marks empty in struct value
let _optionNone = bcs.ser("Option<TransactionType>", {
  none: true,
});

// some now contains a value of type TransactionType
let _optionTx = bcs.ser("Option<TransactionType>", {
  some: {
    single: [1, 2, 3, 4, 5, 6],
  },
});

// same type signature but a different enum invariant - batch
let _optionTxBatch = bcs.ser("Option<TransactionType>", {
  some: {
    batch: [
      [1, 2, 3, 4, 5, 6],
      [1, 2, 3, 4, 5, 6],
    ],
  },
});
```

### Inline (de)serialization

Sometimes it is useful to get a value without registering a new struct. For that inline struct definition can be used.

> Nested struct definitions are not yet supported, only first level properties can be used (but they can reference any type, including other struct types).

```ts
// Example: Inline Struct
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// Some value we want to serialize
const coin = {
  id: "0000000000000000000000000000000000000000000000000000000000000005",
  value: 1111333333222n,
};

// Instead of defining a type we pass struct schema as the first argument
let coin_bytes = bcs.ser({ id: BCS.ADDRESS, value: BCS.U64 }, coin).toBytes();

// Same with deserialization
let coin_restored = bcs.de({ id: BCS.ADDRESS, value: BCS.U64 }, coin_bytes);

console.assert(coin.id == coin_restored.id, "`id` must match");
console.assert(coin.value == coin_restored.value, "`value` must match");
```

## Aligning schema with Move

Currently, main applications of this library are:

1. Serializing transactions and data passed into a transaction
2. Deserializing onchain data for performance and formatting reasons
3. Deserializing events

In this library, all of the primitive Move types are present as built-ins, however, there's a set of special types in Sui which can be simplified to a primitive.

```rust
// Definition in Move which we want to read in JS
module me::example {
    struct Metadata has store {
        name: std::ascii::String,
    }

    struct ChainObject has key {
        id: sui::object::UID,
        owner: address,
        meta: Metadata
    }
    // ...
}
```

Definition for the above should be the following:

```ts
// Example: Simplifying UID
import { BCS, getSuiMoveConfig } from "@mysten/bcs";
const bcs = new BCS(getSuiMoveConfig());

// If there's a deep nested struct we can ignore Move type
// structure and use only the value.
bcs.registerAlias("UID", BCS.ADDRESS);

// Simply follow the definition onchain
bcs.registerStructType("Metadata", {
  name: BCS.STRING,
});

// Same for the main object that we intend to read
bcs.registerStructType("ChainObject", {
  id: "UID",
  owner: BCS.ADDRESS,
  meta: "Metadata",
});
```

<details><summary>See definition of the UID here</summary>
<pre>
struct UID has store {
    id: ID
}

struct ID has store, copy, drop {
bytes: address
}

// { id: { bytes: '0x.....' } }

</pre>
</details>
