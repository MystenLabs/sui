import {
    openTransportReplayer,
    RecordStore,
} from '@ledgerhq/hw-transport-mocker';
import { test, expect } from 'vitest';
import Sui from '../src/Sui';

test('Sui init', async () => {
    const transport = await openTransportReplayer(RecordStore.fromString(''));
    const pkt = new Sui(transport);
    expect(pkt).not.toBe(undefined);
});
