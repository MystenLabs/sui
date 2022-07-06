// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS as bcs, fromB64, toB64 } from './../src/index';
import { BN } from 'bn.js';

describe('Move bcs', () => {
  it('should de/ser primitives: u8', () => {
    expect(bcs.de(bcs.U8, fromB64('AQ=='))).toEqual(new BN(1));
    expect(bcs.de('u8', fromB64('AA=='))).toEqual(new BN(0));
  });

  it('should ser/de u64', () => {
    const exp = 'AO/Nq3hWNBI=';
    const num = BigInt('1311768467750121216');
    const set = bcs.ser('u64', num).toBytes();

    expect(toB64(set)).toEqual(exp);
    expect(bcs.de('u64', fromB64(exp))).toEqual(
      new BN('1311768467750121216')
    );
  });

  it('should ser/de u128', () => {
    const sample = 'AO9ld3CFjD48AAAAAAAAAA==';
    const num = BigInt('1111311768467750121216');

    expect(bcs.de('u128', fromB64(sample)).toString(10)).toEqual(
      '1111311768467750121216'
    );
    expect(bcs.ser('u128', num).toString('base64')).toEqual(sample);
  });

  it('should de/ser custom objects', () => {
    bcs.registerStructType('Coin', {
      value: bcs.U64,
      owner: bcs.STRING,
      is_locked: bcs.BOOL,
    });

    const rustBcs = 'gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA';
    const expected = {
      owner: 'Big Wallet Guy',
      value: new BN('412412400000', 10),
      is_locked: false,
    };

    const setBytes = bcs.ser('Coin', expected);

    expect(bcs.de('Coin', fromB64(rustBcs))).toEqual(expected);
    expect(setBytes.toString('base64')).toEqual(rustBcs);
  });

  it('should de/ser vectors', () => {
    bcs.registerVectorType('vector<u8>', 'u8');

    // Rust-bcs generated vector with 1000 u8 elements (FF)
    const sample = largebcsVec();

    // deserialize data with JS
    const deserialized = bcs.de('vector<u8>', fromB64(sample));

    // create the same vec with 1000 elements
    let arr = Array.from(Array(1000)).map(() => 255);
    const serialized = bcs.ser('vector<u8>', arr);

    expect(deserialized.length).toEqual(1000);
    expect(serialized.toString('base64')).toEqual(largebcsVec());
  });

  it('should de/ser enums', () => {
    bcs.registerStructType('Coin', { value: 'u64' });
    bcs.registerVectorType('vector<Coin>', 'Coin');
    bcs.registerEnumType('Enum', {
      single: 'Coin',
      multi: 'vector<Coin>',
    });

    // prepare 2 examples from Rust bcs
    let example1 = fromB64('AICWmAAAAAAA');
    let example2 = fromB64('AQIBAAAAAAAAAAIAAAAAAAAA');

    // serialize 2 objects with the same data and signature
    let set1 = bcs.ser('Enum', { single: { value: 10000000 } }).toBytes();
    let set2 = bcs.ser('Enum', {
      multi: [{ value: 1 }, { value: 2 }],
    }).toBytes();

    // deserialize and compare results
    expect(bcs.de('Enum', example1)).toEqual(bcs.de('Enum', set1));
    expect(bcs.de('Enum', example2)).toEqual(bcs.de('Enum', set2));
  });

  it('should de/ser addresses', () => {
    // Move Kitty example:
    // Wallet { kitties: vector<Kitty>, owner: address }
    // Kitty { id: 'u8' }

    bcs.registerAddressType('address', 16); // Move has 16/20/32 byte addresses

    bcs.registerStructType('Kitty', { id: 'u8' });
    bcs.registerVectorType('vector<Kitty>', 'Kitty');
    bcs.registerStructType('Wallet', {
      kitties: 'vector<Kitty>',
      owner: 'address',
    });

    // Generated with Move CLI i.e. on the Move side
    let sample = 'AgECAAAAAAAAAAAAAAAAAMD/7g==';
    let data = bcs.de('Wallet', fromB64(sample));

    expect(data.kitties).toHaveLength(2);
    expect(data.owner).toEqual('00000000000000000000000000c0ffee');
  });
});

// @ts-ignore

function largebcsVec(): string {
  return '6Af/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////';
}
