// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BcsReader, fromHEX, toB58, toHEX, TypeName } from '@mysten/bcs';
import { it, expect } from 'vitest';
import {
  builder,
  PROGRAMMABLE_CALL,
  MoveCallCommand,
  ENUM_KIND,
  COMMAND,
  TransferObjectsCommand,
  CALL_ARG,
} from '..';
import { normalizeSuiAddress, SuiObjectRef } from '../../types';

// Oooh-weeee we nailed it!
it('can serialize simplified programmable call struct', () => {
  const moveCall: MoveCallCommand = {
    kind: 'MoveCall',
    target: '0x2::display::new',
    typeArguments: ['0x6::capy::Capy'],
    arguments: [
      { kind: 'GasCoin' },
      {
        kind: 'NestedResult',
        index: 0,
        resultIndex: 1,
      },
      { kind: 'Input', index: 3 },
      { kind: 'Result', index: 1 },
    ],
  };

  const bytes = builder.ser(PROGRAMMABLE_CALL, moveCall).toBytes();
  const result: MoveCallCommand = builder.de(PROGRAMMABLE_CALL, bytes);

  // since we normalize addresses when (de)serializing, the returned value differs
  // only check the module and the function; ignore address comparison (it's not an issue
  // with non-0x2 addresses).
  expect(result.arguments).toEqual(moveCall.arguments);
  expect(result.target.split('::').slice(1)).toEqual(
    moveCall.target.split('::').slice(1),
  );
  expect(result.typeArguments[0].split('::').slice(1)).toEqual(
    moveCall.typeArguments[0].split('::').slice(1),
  );
});

it('can serialize enum with "kind" property', () => {
  const command = {
    kind: 'TransferObjects',
    objects: [],
    address: { kind: 'Input', index: 0 },
  };

  const bytes = builder.ser(COMMAND, command).toBytes();
  const result: TransferObjectsCommand = builder.de(COMMAND, bytes);

  expect(result).toEqual(command);
});

function ref(): SuiObjectRef {
  return {
    objectId: (Math.random() * 100000).toFixed(0).padEnd(40, '0'),
    digest: toB58(new Uint8Array([0,1,2,3,4,5,6,7,8,9,0,1,2,3,4,5,6,7,8,9])),
    version: +(Math.random() * 10000).toFixed(0),
  };
}

it('can serialize transaction data with a programmable transaction', () => {
  let sui = normalizeSuiAddress('0x2').replace('0x', '');

  let txData = {
    sender: normalizeSuiAddress('0xBAD').replace('0x', ''),
    expiration: { None: null },
    gasData: {
      payment: ref(),
      owner: sui,
      price: 1,
      budget: 1000000n,
    },
    kind: {
      Single: {
        ProgrammableTransaction: {
          inputs: [
            // first argument is the publisher object
            { Object: { ImmOrOwned: ref() } },
            // second argument is a vector of names
            // {
            //   Pure: builder
            //     .ser('vector<string>', ['name', 'description', 'img_url'])
            //     .toBytes(),
            // },
            // // third argument is a vector of values
            // {
            //   Pure: builder
            //     .ser('vector<string>', [
            //         'Capy {name}',
            //         'A cute little creature',
            //         'https://api.capy.art/{id}/svg',
            //       ],
            //     )
            //     .toBytes(),
            // },
            // // 4th and last argument is the account address to send display to
            // {
            //     Pure: builder.ser('address', ref().objectId).toBytes()
            // }
          ],
          commands: [
            {
              kind: 'MoveCall',
              target: `${sui}::display::new`,
              typeArguments: [`${sui}::capy::Capy`],
              arguments: [
                // publisher object
                { kind: 'Input', index: 0 },
              ],
            },
            {
              kind: 'MoveCall',
              target: `${sui}::display::add_multiple`,
              typeArguments: [`${sui}::capy::Capy`],
              arguments: [
                // result of the first command
                { kind: 'Result', index: 1 },
                // second argument - vector of names
                { kind: 'Input', index: 1 },
                // third argument - vector of values
                { kind: 'Input', index: 2 },
              ],
            },
            {
              kind: 'MoveCall',
              target: `${sui}::display::update_version`,
              typeArguments: [`${sui}::capy::Capy`],
              arguments: [
                // result of the first command again
                { kind: 'Result', index: 1 },
              ],
            },
            {
              kind: 'TransferObjects',
              objects: [
                // the display object
                { kind: 'Result', index: 1 },
              ],
              // address is also an input
              address: { kind: 'Input', index: 3 },
            },
          ],
        },
      },
    },
  };

  console.log(builder.types.get('ObjectDigest'));

  const value = txData.kind.Single.ProgrammableTransaction.inputs[0];
  const type = [CALL_ARG] as TypeName; // 'TransactionData';

  const bytes = builder.ser(type, value).toBytes();
  console.log(value, toHEX(bytes));

  // 01007627900000000000000000000000000000000000b80e000000000000080000000000000000
  // length is 78 / 2 = 39 bytes

  // 1st byte - order byte of the CallArg enum - 1 (1)
  // 2nd byte - order byte of the ObjectArg enum - 0 (1 + 1)
  // 20 bytes of the address (2 + 20)
  // version 8 bytes (22 + 8)
  // 8 bytes of the version + 1 byte length (30 + 9)
  // total 39 bytes -> should be correct.

  console.log('len', bytes.length);

  const reader = new BcsReader(bytes);
  expect(reader.read8()).toEqual(1);
  expect(reader.read8()).toEqual(0);
  expect(reader.readBytes(20)).toEqual(
    fromHEX(value.Object.ImmOrOwned.objectId),
  );

  expect(reader.read64()).toEqual(BigInt(value.Object.ImmOrOwned.version));
  reader.shift(9);
  //   expect(reader.read64()).toEqual(BigInt(value.Object.ImmOrOwned.version));

  //   console.log('origin', JSON.stringify(value, null, 2));
  //   console.log('result', JSON.stringify(result, null, 2));
  const result = builder.de(type, bytes);
  expect(result).toEqual(value);
});
