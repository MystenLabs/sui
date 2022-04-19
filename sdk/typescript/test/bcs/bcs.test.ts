import { MoveBCS as BCS } from '../../src/bcs';
import { Base64DataBuffer as B64 } from '../../src';
import { BN } from 'bn.js';

describe('Move BCS', () => {
    it('should de/ser primitives: u8', () => {
        expect(BCS.de(BCS.U8, new B64('AQ==').getData())).toEqual(new BN(1));;
        expect(BCS.de('u8', new B64('AA==').getData())).toEqual(new BN(0));;
    });

    it('should de/ser custom objects', () => {
        BCS.registerStructType('Coin', {
            value: BCS.U64,
            owner: BCS.STRING,
            is_locked: BCS.BOOL
        });

        const rustBcs = new B64('gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA').getData();
        const expected = {
            owner: 'Big Wallet Guy',
            value: new BN('412412400000', 10),
            is_locked: false
        };

        expect(BCS.de('Coin', rustBcs)).toEqual(expected);
    });
});
