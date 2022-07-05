# Move BCS

This library implements [Binary Canonical Serialization (BCS)](https://github.com/diem/bcs) in JavaScript, making BCS available in both Browser and NodeJS environments.

## Feature set

- Move's primitive types de/serialization: u8, u64, u128, bool
- Ability to define custom types such as `vector` or `struct`
- Extendable and allows registering any custom types (e.g. vectors of structs)
- Custom addresses length. Example: `BCS.registerAddressType('Address', 20, 'hex')` - 20 bytes;

## Examples

### Working with primitive types

To deserialize data, use a `BCS.de(type: string, data: string)`. Type parameter is a name of the type; data is a BCS encoded as hex.

```js
import { BCS } from '@mysten/bcs';

// BCS has a set of built ins:
// U8, U32, U64, U128, BOOL, STRING
console.assert(BCS.U64 === 'u64');
console.assert(BCS.BOOL === 'bool');
console.assert(BCS.STRING === 'string');

const hex = BCS.util.fromHex;

// De/serialization of primitives is included by default;
let u8 = BCS.de(BCS.U8, hex('00')); // '0'
let u32 = BCS.de(BCS.U32, hex('78563412')); // '78563412'
let u64 = BCS.de(BCS.U64, hex('ffffffffffffffff')); // '18446744073709551615'
let u128 = BCS.de(BCS.U128, hex('FFFFFFFF000000000000000000000000')); // '4294967295'
let bool = BCS.de(BCS.BOOL, hex('00')); // false

// There's also a handy built-in for ASCII strings (which are `vector<u8>` under the hood)
let str = BCS.de(BCS.STRING, hex('0a68656c6c6f5f6d6f7665')); // hello_move

// Address support TBD once the best API is figured out;
// let addr = BCS.de(Move.ADDRESS, '0a68656c6c6f5f6d6f7665'); // 0a68656c6c6f5f6d6f7665
```

To serialize any type, use `BCS.ser(type: string, data: any)`. Type parameter is a name of the type to serialize, data is any data, depending on the type (can be object for structs or string for big integers - such as `u128`).

```js
import { BCS } from '@mysten/bcs';

let bcs_u8 = BCS.ser('u8', 255).toBytes(); // uint Array

console.assert(BCS.util.toHex(bcs_u8) === 'ff');

let bcs_ascii = BCS.ser('string', 'hello_move').toBytes();

console.assert(BCS.util.toHex(bcs_ascii) === '0a68656c6c6f5f6d6f7665');
```

### Adding custom types

```js
import { BCS } from '@mysten/bcs';

// Move / Rust struct
// struct Coin {
//   value: u64,
//   owner: vector<u8>, // name // Vec<u8> in Rust
//   is_locked: bool,
// }

BCS.registerStructType('Coin', {
    value: BCS.U64,
    owner: BCS.STRING,
    is_locked: BCS.BOOL
});



// Created in Rust with diem/bcs
let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';

console.log(BCS.de('Coin', BCS.util.fromHex(rust_bcs_str)));

// Let's encode the value as well
let test_ser = BCS.ser('Coin', {
    owner: 'Big Wallet Guy',
    value: '412412400000',
    is_locked: false
});

console.log(test_ser.toBytes());
console.assert(BCS.util.toHex(test_ser.toBytes()) === rust_bcs_str, 'Whoopsie, result mismatch');
```
