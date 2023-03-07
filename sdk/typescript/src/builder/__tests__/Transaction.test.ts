// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import { describe, it, expect } from 'vitest';
import { Transaction, Commands } from '..';
import { Inputs } from '../Inputs';

it('can construct and serialize an empty tranaction', () => {
  const tx = new Transaction();
  expect(() => tx.serialize()).not.toThrow();
});

it('can be serialized and deserialized to the same values', () => {
  const tx = new Transaction();
  tx.add(Commands.SplitCoin(tx.gas, tx.input(100)));
  const serialized = tx.serialize();
  const tx2 = Transaction.from(serialized);
  expect(serialized).toEqual(tx2.serialize());
});

it('allows transfer with the result of split commands', () => {
  const tx = new Transaction();
  const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(100)));
  tx.add(Commands.TransferObjects([coin], tx.input('0x2')));
});

it('supports nested results through either array index or destructuring', () => {
  const tx = new Transaction();
  const registerResult = tx.add(
    Commands.MoveCall({
      package: '0x2',
      function: 'game',
      module: 'register',
      arguments: [],
      typeArguments: [],
    }),
  );

  const [nft, account] = registerResult;

  // NOTE: This might seem silly but destructuring works differently than property access.
  expect(nft).toBe(registerResult[0]);
  expect(account).toBe(registerResult[1]);
});

describe('offline build', () => {
  it('builds an empty transaction offline when provided sufficient data', async () => {
    const tx = setup();
    await tx.build();
  });

  it('supports epoch expiration', async () => {
    const tx = setup();
    tx.setExpiration({ Epoch: 1 });
    await tx.build();
  });

  it('builds a split command', async () => {
    const tx = setup();
    tx.add(Commands.SplitCoin(tx.gas, tx.input(Inputs.Pure('u64', 100))));
    await tx.build();
  });

  it('infers the type of inputs', async () => {
    const tx = setup();
    tx.add(Commands.SplitCoin(tx.gas, tx.input(100)));
    await tx.build();
  });

  it('builds a more complex interaction', async () => {
    const tx = setup();
    const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(100)));
    tx.add(
      Commands.MergeCoins(tx.gas, [coin, tx.input(Inputs.ObjectRef(ref()))]),
    );
    tx.add(
      Commands.MoveCall({
        target: '0x2::devnet_nft::mint',
        typeArguments: [],
        arguments: [
          tx.input(Inputs.Pure('string', 'foo')),
          tx.input(Inputs.Pure('string', 'bar')),
          tx.input(Inputs.Pure('string', 'baz')),
        ],
      }),
    );
    await tx.build();
  });
});

// describe('online building (TODO: move to e2e)', () => {
//   it('builds', async () => {
//     const tx = setup();
//     const provider = new JsonRpcProvider(localnetConnection);
//     const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(100)));
//     tx.add(
//       Commands.MergeCoins(tx.gas, [coin, tx.input(Inputs.ObjectRef(ref()))]),
//     );
//     tx.add(
//       Commands.MoveCall({
//         target: '0x2::devnet_nft::mint',
//         typeArguments: [],
//         arguments: [tx.input('foo'), tx.input('bar'), tx.input('baz')],
//       }),
//     );
//     await tx.build({ provider });
//   });
// });

function ref(): { objectId: string; version: bigint; digest: string } {
  return {
    objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
    version: BigInt((Math.random() * 10000).toFixed(0)),
    digest: toB58(
      new Uint8Array([
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
      ]),
    ),
  };
}

function setup() {
  const tx = new Transaction();
  tx.setSender('0x2');
  tx.setGasPrice(1);
  tx.setGasBudget(1);
  tx.setGasPayment([ref()]);
  return tx;
}
