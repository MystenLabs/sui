import { MoveBCS as BCS } from '../../src/bcs';
import { Base64DataBuffer as B64 } from '../../src';
import { BN } from 'bn.js';

describe('Move BCS', () => {
    it('should de/serialize primitives: u8', () => {
        expect(BCS.de(BCS.U8, new B64('AQ==').getData())).toEqual(new BN(1));;
        expect(BCS.de('u8', new B64('AA==').getData())).toEqual(new BN(0));;
    });
});
