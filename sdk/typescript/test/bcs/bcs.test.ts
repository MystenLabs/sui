// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '../../src/bcs';
import { Base64DataBuffer as B64 } from '../../src';
// import { HexDataBuffer as HEX } from '../../src';
import * as BN from 'bn.js';

describe('Move BCS', () => {
    it('should de/ser primitives: u8', () => {
        expect(BCS.de(BCS.U8, new B64('AQ==').getData())).toEqual(new BN.BN(1));;
        expect(BCS.de('u8', new B64('AA==').getData())).toEqual(new BN.BN(0));;
    });

    it('should ser/de u64', () => {
        const exp = 'AO/Nq3hWNBI=';
        const num = BigInt('1311768467750121216');
        const ser = BCS.ser('u64', num).toBytes();

        expect(new B64(ser).toString()).toEqual(exp);
        expect(BCS.de('u64', new B64(exp).getData())).toEqual(new BN.BN('1311768467750121216'));
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
            value: new BN.BN('412412400000', 10),
            is_locked: false
        };

        const serBytes = BCS.ser('Coin', expected);

        expect(BCS.de('Coin', rustBcs.getData())).toEqual(expected);
        expect(serBytes.toString('base64')).toEqual(rustBcs.toString());
    });

    it('should de/ser vectors', () => {
        BCS.registerVectorType('vector<u8>', 'u8');

        // Rust-BCS generated vector with 1000 u8 elements (FF)
        const sample = new B64(largeBCSVec());

        // deserialize data with JS
        const deserialized = BCS.de('vector<u8>', sample.getData());

        // create the same vec with 1000 elements
        let arr = Array.from(Array(1000)).map(() => 255);
        const serialized = BCS.ser('vector<u8>', arr);

        expect(deserialized.length).toEqual(1000);
        expect(serialized.toString('base64')).toEqual(largeBCSVec());
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

    it('should de/ser addresses', () => {
        // Move Kitty example:
        // Wallet { kitties: vector<Kitty>, owner: address }
        // Kitty { id: 'u8' }

        BCS.registerAddressType('address', 16); // Move has 16/20/32 byte addresses

        BCS.registerStructType('Kitty', { id: 'u8' });
        BCS.registerVectorType('vector<Kitty>', 'Kitty');
        BCS.registerStructType('Wallet', {
            kitties: 'vector<Kitty>',
            owner: 'address'
        });

        // Generated with Move CLI i.e. on the Move side
        let sample = 'AgECAAAAAAAAAAAAAAAAAMD/7g==';
        let data = BCS.de('Wallet', new B64(sample).getData());

        expect(data.kitties).toHaveLength(2);
        expect(data.owner).toEqual('00000000000000000000000000c0ffee');
    });

    it('should de/ser TransactionData', () => {
        BCS.registerAddressType('object_id', 20);
        BCS.registerVectorType('sui_address', 'u8');
        BCS.registerVectorType('object_digest', 'u8') ;

        BCS.registerVectorType('vector<u8>', 'u8');
        BCS.registerStructType('ObjectRef', {
            ObjectId: 'object_id',
            SequenceNumber: 'u64',
            ObjectDigest: 'object_digest'
        });

        BCS.registerStructType('Transfer', {
            recipient: 'sui_address', // 'sui_address' - not actually an address,
            object_ref: 'ObjectRef',
        });

        BCS.registerVectorType('vector<vector<u8>>', 'vector<u8>');
        BCS.registerStructType('MoveModulePublish', { modules: 'vector<vector<u8>>' });
        BCS.registerStructType('Identifier', { value: 'string' });
        BCS.registerEnumType('TypeTag', {
            bool: null,
            u8: null,
            u64: null,
            u128: null,
            address: null,
            signer: null,
            vector: 'TypeTag',
            struct: 'StructTag',
        });

        BCS.registerVectorType('vector<TypeTag>', 'TypeTag');
        BCS.registerStructType('StructTag', {
            address: 'sui_address',
            module: 'Identifier',
            name: 'Identifier',
            type_args: 'vector<TypeTag>'
        });

        BCS.registerVectorType('vector<ObjectRef>', 'ObjectRef');
        BCS.registerVectorType('vector<sui_address>', 'sui_address');

        BCS.registerStructType('MoveCall', {
            package: 'ObjectRef',
            module: 'Identifier', //
            function: 'Identifier', //
            type_arguments: 'vector<TypeTag>',
            object_arguments: 'vector<ObjectRef>',
            shared_object_arguments: 'vector<sui_address>',
            pure_arguments: 'vector<vector<u8>>',
        });

        BCS.registerEnumType('SingleTransactionKind', {
            transfer: 'Transfer',
            publish: 'MoveModulePublish',
            call: 'MoveCall'
        });

        BCS.registerVectorType('vector<SingleTransactionKind>', 'SingleTransactionKind');
        BCS.registerEnumType('TransactionKind', {
            Single: 'SingleTransactionKind',
            Batch: 'vector<SingleTransactionKind>'
        });

        BCS.registerStructType('TransactionData', {
            kind: 'TransactionKind',
            sender: 'sui_address',
            gas_payment: 'ObjectRef',
            gas_budget: 'u64'
        });

        let de = BCS.de('TransactionData', new B64(transactionData().transfer).getData());
        expect(BCS.ser('TransactionData', de).toString('base64')).toEqual(transactionData().transfer);

        // console.log(
        //     JSON.stringify(
        //         BCS.de('TransactionData', new B64(transactionData().transfer).getData()),
        //         null,
        //         4
        //     )
        // );

    });
});

function largeBCSVec(): string {
    return '6Af/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////';
}

function transactionData(): {
    transfer: string,
    move_call: string,
    module_publish: string
} {
    return {
        transfer: 'AAAUICAgICAgICAgICAgICAgICAgICCfe2xvEE25eefHZHcvJgNxzAZSAQAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFESprl7V7fccfAGyQ+gaKY0D8MUxN4LD3Mtw2NCRYHZG4Kkk4EtcQTEAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAnAAAAAAAA',
        move_call: '',
        module_publish: '',
    };
}
