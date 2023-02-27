// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Transaction, Commands } from './';

const tx = new Transaction({ inputs: ['amount', 'address'] });
const coin = tx.add(Commands.Split(tx.gas(), tx.input('amount')));
tx.add(Commands.TransferObjects([coin.result()], tx.input('address')));
tx.add(
  Commands.MoveCall({
    package: '0x2',
    function: 'game',
    module: 'register',
    arguments: [],
    typeArguments: [],
  }),
);

tx.setGasPrice(BigInt(10));

tx.provideInputs({
  amount: 100,
  address: '0x2',
});

const serialized = tx.serialize();
console.log(serialized);

const tx2 = Transaction.from(serialized);
console.log(tx2.serialize());
