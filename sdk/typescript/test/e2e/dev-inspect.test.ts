// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { RawSigner, Transaction, Commands } from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Test dev inspect', () => {
  let toolbox: TestToolbox;
  let packageId: string;

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath = __dirname + '/./data/serializer';
    ({ packageId } = await publishPackage(packagePath));
  });

  // TODO: This is skipped because this fails currently.
  it.skip('Dev inspect split + transfer', async () => {
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(10)));
    tx.add(Commands.TransferObjects([coin], tx.input(toolbox.address())));
    await validateDevInspectTransaction(toolbox.signer, tx, 'success');
  });

  it('Move Call that returns struct', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();

    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    const obj = tx.add(
      Commands.MoveCall({
        target: `${packageId}::serializer_tests::return_struct`,
        typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>'],
        arguments: [tx.input(coins[0].objectId)],
      }),
    );

    // TODO: Ideally dev inspect transactions wouldn't need this, but they do for now
    tx.add(Commands.TransferObjects([obj], tx.input(toolbox.address())));

    await validateDevInspectTransaction(toolbox.signer, tx, 'success');
  });

  it('Move Call that aborts', async () => {
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    tx.add(
      Commands.MoveCall({
        target: `${packageId}::serializer_tests::test_abort`,
        typeArguments: [],
        arguments: [],
      }),
    );

    await validateDevInspectTransaction(toolbox.signer, tx, 'failure');
  });
});

async function validateDevInspectTransaction(
  signer: RawSigner,
  txn: Transaction,
  status: 'success' | 'failure',
) {
  const result = await signer.devInspectTransaction(txn);
  expect(result.effects.status.status).toEqual(status);
}
