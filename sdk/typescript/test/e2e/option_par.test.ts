// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { LocalTxnDataSerializer } from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Test option parameter', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Test request_add_delegation_mul_coin', async () => {
    const sender = await toolbox.address();
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(toolbox.address());
    const serializer = new LocalTxnDataSerializer(toolbox.provider);
    const tx = {
        packageObjectId: '0x2',
        module: 'sui_system',
        function: 'request_add_delegation_mul_coin',
        typeArguments: [],
        arguments: [
            '0x0000000000000000000000000000000000000005',
            [coins[0].objectId], //delegate_stakes
            ['2000'], // stake_amount
            '0x5d06f37654f11cdd27179088fcfeadaab21e13ef', //validator_address
        ],
        gasPayment: '0x30c77a83f10b0a5f44db303b437108a4e33d7b89',
        gasBudget: 15000,
    };
    const serializedTx = await serializer.serializeToBytes(sender, { kind: 'moveCall', data: tx });
    expect(serializedTx).toBeTruthy;
  });
});