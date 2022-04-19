# Move BCS

This library implements [Binary Canonical Serialization (BCS)](https://github.com/diem/bcs) in JavaScript, making BCS available in both Browser and NodeJS environments.

## Feature set

- Move's primitive types de/serialization: u8, u32, u64, u128, bool
- Ability to define custom types such as `vector` or `struct`
- Extendable and allows registering any custom types (e.g. vectors of structs)

## Examples

### Working with primitive types

To deserialize data, use a `MoveBCS.de(type: string, data: string)`. Type parameter is a name of the type; data is a BCS encoded as hex. 

```js
// MoveBCS has a set of built ins:
// U8, U32, U64, U128, BOOL, STRING
console.assert(MoveBCS.U64 === 'u64');
console.assert(MoveBCS.BOOL === 'bool');
console.assert(MoveBCS.STRING === 'string');

const hex = MoveBCS.util.fromHex;

// De/serialization of primitives is included by default;
let u8 = MoveBCS.de(MoveBCS.U8, hex('00')); // '0'
let u32 = MoveBCS.de(MoveBCS.U32, hex('78563412')); // '78563412'
let u64 = MoveBCS.de(MoveBCS.U64, hex('ffffffffffffffff')); // '18446744073709551615'
let u128 = MoveBCS.de(MoveBCS.U128, hex('FFFFFFFF000000000000000000000000')); // '4294967295'
let bool = MoveBCS.de(MoveBCS.BOOL, hex('00')); // false

// There's also a handy built-in for ASCII strings (which are `vector<u8>` under the hood)
let str = MoveBCS.de(MoveBCS.STRING, hex('0a68656c6c6f5f6d6f7665')); // hello_move

// Address support TBD once the best API is figured out;
// let addr = MoveBCS.de(Move.ADDRESS, '0a68656c6c6f5f6d6f7665'); // 0a68656c6c6f5f6d6f7665
```

To serialize any type, use `MoveBCS.ser(type: string, data: any)`. Type parameter is a name of the type to serialize, data is any data, depending on the type (can be object for structs or string for big integers - such as `u128`).

```js
let bcs_u8 = MoveBCS.ser('u8', 255).toBytes(); // uint Array

console.assert(MoveBCS.util.toHex(bcs_u8) === 'ff');

let bcs_ascii = MoveBCS.ser('string', 'hello_move').toBytes();

console.assert(MoveBCS.util.toHex(bcs_ascii) === '0a68656c6c6f5f6d6f7665');
```

### Adding custom types

TBD

```js
// Move / Rust struct
// struct Coin {
//   value: u64,
//   owner: vector<u8>, // name // Vec<u8> in Rust
//   is_locked: bool,
// }

MoveBCS.registerStructType('Coin', {
    value: MoveBCS.U64,
    owner: MoveBCS.STRING,
    is_locked: MoveBCS.BOOL
});



// Created in Rust with diem/bcs 
let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';

console.log(MoveBCS.de('Coin', MoveBCS.util.fromHex(rust_bcs_str)));

// Let's encode the value as well
let test_ser = MoveBCS.ser('Coin', {
    owner: 'Big Wallet Guy',
    value: '412412400000',
    is_locked: false
});

console.log(test_ser.toBytes());
console.assert(MoveBCS.util.toHex(test_ser.toBytes()) === rust_bcs_str, 'Whoopsie, result mismatch');
```

## TODO

- [ ] Add support for `u128` serialization (deserialization is ready)
- [ ] Improve frontend wrapper for better DevEx based on usage
- [ ] Figure out an obvious way to add support for addresses
<!-- Addresses differ on different platforms -->
