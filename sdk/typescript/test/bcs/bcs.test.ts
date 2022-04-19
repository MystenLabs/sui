// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '../../src/bcs';
import { Base64DataBuffer as B64 } from '../../src';
import { BN } from 'bn.js';

describe('Move BCS', () => {
    it('should de/ser primitives: u8', () => {
        expect(BCS.de(BCS.U8, new B64('AQ==').getData())).toEqual(new BN(1));;
        expect(BCS.de('u8', new B64('AA==').getData())).toEqual(new BN(0));;
    });

    it('should ser/de u64', () => {
        const exp = 'AO/Nq3hWNBI=';
        const num = BigInt('1311768467750121216');
        const ser = BCS.ser('u64', num).toBytes();

        expect(new B64(ser).toString()).toEqual(exp);
        expect(BCS.de('u64', new B64(exp).getData())).toEqual(new BN('1311768467750121216'));
    });

    it('should ser/de u128', () => {
        const sample = new B64('AO9ld3CFjD48AAAAAAAAAA==');
        const num = BigInt('1111311768467750121216');

        expect(BCS.de('u128', sample.getData()).toString(10)).toEqual('1111311768467750121216');
        expect(new B64(BCS.ser('u128', num).toBytes()).toString()).toEqual(sample.toString())
    });

    it('should de/ser custom objects', () => {
        BCS.registerStructType('Coin', {
            value: BCS.U64,
            owner: BCS.STRING,
            is_locked: BCS.BOOL
        });

        const rustBcs = new B64('gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA');
        const expected = {
            owner: 'Big Wallet Guy',
            value: new BN('412412400000', 10),
            is_locked: false
        };

        const serBytes = BCS.ser('Coin', expected).toBytes();

        expect(BCS.de('Coin', rustBcs.getData())).toEqual(expected);
        expect(new B64(serBytes).toString()).toEqual(rustBcs.toString());
    });

    it('should de/ser vectors', () => {
        BCS.registerVectorType('vector<u8>', 'u8');

        // Rust-BCS generated vector with 1000 u8 elements (FF)
        const sample = new B64(largeBCSVec());

        // deserialize data with JS
        const deserialized = BCS.de('vector<u8>', sample.getData());

        // create the same vec with 1000 elements
        let arr = Array.from(Array(1000)).map(() => 255);
        const serialized = BCS.ser('vector<u8>', arr).toBytes();

        expect(deserialized.length).toEqual(1000);
        expect(new B64(serialized).toString()).toEqual(largeBCSVec());
    });

    it('should de/ser enums', () => {
        BCS.registerStructType('Coin', { value: 'u64' });
        BCS.registerVectorType('vector<Coin>', 'Coin');
        BCS.registerEnumType('Enum', {
            single: 'Coin',
            multi: 'vector<Coin>'
        });

        // prepare 2 examples from Rust BCS
        let example1 = new B64('AICWmAAAAAAA');
        let example2 = new B64('AQIBAAAAAAAAAAIAAAAAAAAA');

        // serialize 2 objects with the same data and signature
        let ser1 = BCS.ser('Enum', { single: { value: 10000000 } }).toBytes();
        let ser2 = BCS.ser('Enum', { multi: [ { value: 1 }, { value: 2 } ] }).toBytes();

        // deserialize and compare results
        expect(BCS.de('Enum', example1.getData())).toEqual(BCS.de('Enum', ser1));
        expect(BCS.de('Enum', example2.getData())).toEqual(BCS.de('Enum', ser2));

    });
});


function largeBCSVec(): string {
    return '6Af/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////';
}

// function transactionData(): string {
//     return 'VHJhbnNhY3Rpb25EYXRhOjoAAQLbA6Ec6wsFAAAACQEACAIIFAMcNwRTCgVdcgfPAXQIwwIoCusCBQzwAkIAAAEBAQIBAwAAAgABBAwBAAEBAQwBAAEDAwIAAAUAAQAABgIBAAAHAwQAAAgFAQABBQcBAQABCgkKAQIDCwsMAAIMDQEBCAEHDg8BAAEIEAEBAAQGBQYHCAgGCQYDBwsBAQgACwIBCAAHCAMAAQcIAwMHCwEBCAADBwgDAQsCAQgAAwsBAQgABQcIAwEIAAILAgEJAAcLAQEJAAELAQEIAAIJAAcIAwELAQEJAAEGCAMBBQIJAAUDAwcLAQEJAAcIAwELAgEJAAILAQEJAAUHTUFOQUdFRARDb2luCFRyYW5zZmVyCVR4Q29udGV4dAtUcmVhc3VyeUNhcARidXJuBGluaXQEbWludAx0cmFuc2Zlcl9jYXALZHVtbXlfZmllbGQPY3JlYXRlX2N1cnJlbmN5BnNlbmRlcgh0cmFuc2ZlcgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAgEJAQABAAABBAsBCwA4AAIBAAAACAsJEgAKADgBDAELAQsALhEGOAICAgEAAAEFCwELAAsCOAMCAwEAAAEECwALATgEAgD5BqEc6wsFAAAACwEADgIOJAMyWQSLARwFpwGrAQfSAukBCLsEKAbjBAoK7QQdDIoFswENvQYGAAAAAQECAQMBBAEFAQYAAAIAAAcIAAICDAEAAQQEAgABAQIABgYCAAMQBAACEgwBAAEACAABAAAJAgMAAAoEBQAACwYHAAAMBAUAAA0EBQACFQoFAQACCAsDAQACFg0OAQACFxESAQIGGAITAAIZAg4BAAUaFQMBCAIbFgMBAAILFw4BAAINGAUBAAYJBwkIDAgPCQkLDAsPDBQGDwYMDQwNDw4JDwkDBwgBCwIBCAAHCAUCCwIBCAMLA';
// }
