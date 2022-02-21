import { instanceOfDataType } from './TransactionResult';

describe('tests for Type Guard', () => {
    test('correct object passes', () => {
        const entry = {
            id: 'A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
            sender: '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            status: 'success',
            created: [
                '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            ],
        };
        expect(instanceOfDataType(entry)).toBe(true);
    });

    test('undefined object fails', () => {
        //If find finds no matching value, undefined is returned
        const entry = undefined;
        expect(instanceOfDataType(entry)).toBe(false);
    });

    test('necessary field missing fails', () => {
        const entryNoStatus = {
            id: 'A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
            sender: '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            created: [
                '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            ],
        };
        expect(instanceOfDataType(entryNoStatus)).toBe(false);
    });

    test('no created/mutated/deleted passes', () => {
        const entry = {
            id: 'A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
            sender: '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            status: 'fail',
        };

        expect(instanceOfDataType(entry)).toBe(true);
    });
});
