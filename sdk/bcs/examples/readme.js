// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { BCS, getSuiMoveSchema } = require('./../dist');


// bcs_v1.ts
// export const bcs = new BCS({
//
//
// });

{
    let bcs = new BCS(getSuiMoveSchema());

    bcs.registerVectorType('bytes', 'u8');



    {
        bcs.registerEnumType('Option<T>', {
            none: null,
            some: 'T'
        });

        bcs.registerStructType('Damir<T>', {
            age: 'T'
        });

        let ser = bcs.ser('Option<Damir<u64>>', { some: { age: 10000 } }).toString('hex');
        let de = bcs.de('Option<Damir<u64>>', ser, 'hex');

        console.log(de);
    };


    console.log(
        bcs.de(
            'bytes',
            bcs.ser(
                'bytes',
                [1,2,3,4,5]
            ).toString('hex'),
            'hex'
        )
    );

    bcs.registerType(
        'number',
        (writer, data) => bcs.getTypeInterface('u8')._encodeRaw(writer, data),
        (reader) => bcs.getTypeInterface('u8')._decodeRaw(reader),
        () => true
    );

    bcs.registerStructType('TransactionData<T, DS>', {

    });

    bcs.registerStructType('Damir<T>', {
        age: 'u8'
    });

    bcs.registerStructType('Sam<T, S>', {
        me: 'S',
        you: 'Damir<T>'
    });

    {
        let ser = bcs.ser('Sam<u8, bool>', {
            me: true,
            you: { age: 10 }
        }).toString('hex');

        let de = bcs.de('Sam<u8, bool>', ser, 'hex');
        console.log(ser);
        console.log(de);
    };

    return;

    // let bcs = new BCS(getRustSchema());

    // when we register this type, we need to find a generic parameter.
    // for that we can use genericSeparator and test a string. Simplest solution
    // would be to run this check:
    // "typeSig".includes(genericSeparator[0] + generic + genericSeparator[1]);
    // if generic is not specified - abort. I don't like that strings are used as
    // keys that much.
    //  - we have two scenarios: `bcs.de('Option<u8>')`
    //  - ...and bcs.de('Option<T>', 'T');
    // the latter is way more convenient in code... but I might prefer the former for
    // starters. This is going to be a breaking change which we'll see when we first
    // start to work with it.
    bcs.registerEnumType('Option<T>', {
        none: null,
        some: 'T'
    });

    // on the other hand when we define a new type:
    bcs.registerStructType('Beep<T>', {
        name: 'string',
        item: 'Option<T>'
    });

    // what we actually want to have here is substitute `u8 -> T`.
    // what do we do for that to be possible?
    //

    bcs.ser('Option<u8>', '00', 'hex');

    return;
};

{
    const bcs = new BCS({});

    // BCS has a set of built ins:
    // U8, U32, U64, U128, BOOL, STRING
    console.assert(BCS.U64 === 'u64');
    console.assert(BCS.BOOL === 'bool');
    console.assert(BCS.STRING === 'string');

    // De/serialization of primitives is included by default;
    let u8 = bcs.de(BCS.U8, '00', 'hex'); // '0'
    let u32 = bcs.de(BCS.U32, '78563412', 'hex'); // '78563412'
    let u64 = bcs.de(BCS.U64, 'ffffffffffffffff', 'hex'); // '18446744073709551615'
    let u128 = bcs.de(BCS.U128, 'FFFFFFFF000000000000000000000000', 'hex'); // '4294967295'
    let bool = bcs.de(BCS.BOOL, '00', 'hex'); // false

    // There's also a handy built-in for ASCII strings (which are `vector<u8>` under the hood)
    let str = bcs.de(BCS.STRING, '0a68656c6c6f5f6d6f7665', 'hex'); // hello_move

    console.log(str);
}


{
    const bcs = new BCS({});

    let bcs_u8 = bcs.ser('u8', 255).toString('hex'); // uint Array
    console.assert(bcs_u8 === 'ff');

    let bcs_ascii = bcs.ser('string', 'hello_move').toString('hex');
    console.assert(bcs_ascii === '0a68656c6c6f5f6d6f7665');
}

{
    const bcs = new BCS({});

    // Move / Rust struct
    // struct Coin {
    //   value: u64,
    //   owner: vector<u8>, // name // Vec<u8> in Rust
    //   is_locked: bool,
    // }

    bcs.registerStructType('Coin', {
        value: BCS.U64,
        owner: BCS.STRING,
        is_locked: BCS.BOOL
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
}
