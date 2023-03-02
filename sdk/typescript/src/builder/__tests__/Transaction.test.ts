// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { it, expect } from 'vitest';
import { Transaction, Commands } from '..';

it('can construct and serialize an empty tranaction', () => {
  const tx = new Transaction();
  expect(() => tx.serialize()).not.toThrow();
});

it('can be serialized and deserialized to the same values', () => {
  const tx = new Transaction();
  tx.add(Commands.Split(tx.gas(), tx.createInput(100)));
  const serialized = tx.serialize();
  const tx2 = Transaction.from(serialized);
  expect(serialized).toEqual(tx2.serialize());
});

it('allows transfer with the result of split commands', () => {
  const tx = new Transaction();
  const coin = tx.add(Commands.Split(tx.gas(), tx.createInput(100)));
  tx.add(Commands.TransferObjects([coin], tx.createInput('0x2')));
});

it('supports nested results through either array index or destructuring', () => {
  const tx = new Transaction();
  const registerResult = tx.add(
    Commands.MoveCall({
      target: '0x2::game::register',
      arguments: [],
      typeArguments: [],
    }),
  );

  const [nft, account] = registerResult;

  // NOTE: This might seem silly but destructuring
  expect(nft).toEqual(registerResult[0]);
  expect(account).toEqual(registerResult[1]);
});

it('fails to serialize without setting input values', () => {});

it('correctly serializes with input values provided', () => {});

// const tx = new Transaction({ inputs: ['amount', 'address'] });
// const coin = tx.add(Commands.Split(tx.gas(), tx.input('amount')));
// console.log(coin);
// tx.add(Commands.TransferObjects([coin], tx.input('address')));
// tx.add(
//   Commands.MoveCall({
//     package: '0x2',
//     function: 'game',
//     module: 'register',
//     arguments: [],
//     typeArguments: [],
//   }),
// );

// tx.setGasPrice(BigInt(10));

// tx.provideInputs({
//   amount: 100,
//   address: '0x2',
// });

// const serialized = tx.serialize();
// console.log(serialized);

// const tx2 = Transaction.from(serialized);
// console.log(tx2.serialize());
