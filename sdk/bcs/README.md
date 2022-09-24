# BCS - Binary Canonical Serialization

This library implements [Binary Canonical Serialization (BCS)](https://github.com/diem/bcs) in JavaScript, making BCS available in both Browser and NodeJS environments.

## Feature set

- Move's primitive types de/serialization: u8, u64, u128, bool
- Ability to define custom types such as `vector<T>` or `struct`
- Extendable and allows registering any custom types (e.g. vectors of structs)
- Custom addresses length. Example: `BCS.registerAddressType('Address', 20, 'hex')` - 20 bytes
- Built-in support for enums (and potentially tuples)
- And again - full browser support!

## Examples

At the high level, BCS gives a set of handy abstractions to (de)serialize data.

> Important: by default there's no type `address` in this library. To define it, use `registerAddressType`.
> Also, there's no built-in support for generics yet. For each `vector<T>` you have to define custom type
> using `registerVectorType('vector<u8>', 'u8')`. Default support for vectors is intentionally omitted (for now)
> because of type difference between Rust and Move vector types.

### Struct

In BCS structs are merely sequences of fields, they contain no type information but the order in
which fields are defined. It also means that you can use any field names - they won't affect serialization!
```
bcs.registerStructType(<TYPE>, {
    [<FIELD>]: <FIELD_TYPE>,
    ...
})
```

```js
import { bcs } from "@mysten/bcs";

// MyAddr is an address of 20 bytes; encoded and decoded as HEX
bcs.registerAddressType('MyAddr', 20, 'hex');
bcs.registerStructType('Item', {
    owner: 'MyAddr',
    price: 'u64'
});

// bcs preserves order of fields according to struct definition, so you're free to
// use any order while serializing your structs
let bcs_bytes = bcs.ser('Item', {
    price: '100000000000',
    owner: '9c88e852aa66b346860ada31aa75c6c27695ae4b',
});
let item = bcs.de('Item', bcs_bytes);

console.log(item);
```

### Vector

Vector generics are not supported by default. To use a vector type, add it first:
```
bcs.registerVectorType(<TYPE>, <ELEMENT_TYPE>);
```

```js
import { bcs } from "@mysten/bcs";

bcs.registerVectorType('vector<u8>', 'u8');
let array = bcs.de('vector<u8>', '06010203040506', 'hex'); // [1,2,3,4,5,6];
let again = bcs.ser('vector<u8>', [1,2,3,4,5,6]).toString('hex');

console.assert(again === '06010203040506', 'Whoopsie!');
```

### Address

Even though the way of serializing Move addresses stays the same, the length of the address
varies depending on the network. To register an address type use:
```
bcs.registerAddressType(<TYPE>, <LENGTH>);
```

```js
import { bcs } from "@mysten/bcs";

bcs.registerAddressType('FiveByte', 5);
bcs.registerAddressType('DiemAddress', 20);

let de = bcs.de('FiveBytes', '0x00C0FFEE00', 'hex');
let ser = bcs.ser('DiemAddress', '9c88e852aa66b346860ada31aa75c6c27695ae4b').toString('hex');

console.assert(de === '00c0ffee00', 'Short address mismatch');
console.assert(ser === '9c88e852aa66b346860ada31aa75c6c27695ae4b', 'Long address mismatch');
```

### Primitive types

To deserialize data, use a `BCS.de(type: string, data: Uint8Array)`. Type parameter is a name of the type; data is a BCS encoded as hex.

```js
import { bcs } from '@mysten/bcs';

// BCS has a set of built ins:
// U8, U32, U64, U128, BOOL, STRING
console.assert(bcs.U64 === 'u64');
console.assert(bcs.BOOL === 'bool');
console.assert(bcs.STRING === 'string');

// De/serialization of primitives is included by default;
let u8 = bcs.de(bcs.U8, '00', 'hex'); // '0'
let u32 = bcs.de(bcs.U32, '78563412', 'hex'); // '78563412'
let u64 = bcs.de(bcs.U64, 'ffffffffffffffff', 'hex'); // '18446744073709551615'
let u128 = bcs.de(bcs.U128, 'FFFFFFFF000000000000000000000000', 'hex'); // '4294967295'
let bool = bcs.de(bcs.BOOL, '00', 'hex'); // false

// There's also a handy built-in for ASCII strings (which are `vector<u8>` under the hood)
let str = bcs.de(bcs.STRING, '0a68656c6c6f5f6d6f7665', 'hex'); // hello_move

console.log(str);
```

To serialize any type, use `bcs.ser(type: string, data: any)`. Type parameter is a name of the type to serialize, data is any data, depending on the type (can be object for structs or string for big integers - such as `u128`).

```js
import { bcs } from '@mysten/bcs';

let bcs_u8 = bcs.ser('u8', 255).toString('hex'); // uint Array
console.assert(bcs_u8 === 'ff');

let bcs_ascii = bcs.ser('string', 'hello_move').toString('hex');
console.assert(bcs_ascii === '0a68656c6c6f5f6d6f7665');
```

### Working with Move structs

```js
import { bcs } from '@mysten/bcs';

// Move / Rust struct
// struct Coin {
//   value: u64,
//   owner: vector<u8>, // name // Vec<u8> in Rust
//   is_locked: bool,
// }

bcs.registerStructType('Coin', {
    value: bcs.U64,
    owner: bcs.STRING,
    is_locked: bcs.BOOL
});

// Created in Rust with diem/bcs
let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';

console.log(bcs.de('Coin', rust_bcs_str, 'hex'));

// Let's encode the value as well
let test_ser = bcs.ser('Coin', {
    owner: 'Big Wallet Guy',
    value: '412412400000',
    is_locked: false
});

console.log(test_ser.toBytes());
console.assert(test_ser.toString('hex') === rust_bcs_str, 'Whoopsie, result mismatch');
```
