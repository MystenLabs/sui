// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getObjectId,
  getNewlyCreatedCoinRefsAfterSplit,
  LocalTxnDataSerializer,
  SignableTransaction,
  RawSigner,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe.each([{ useLocalTxnBuilder: false }, { useLocalTxnBuilder: true }])(
  'Test dev inspect',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    let packageId: string;
    let shouldSkip: boolean;

    beforeAll(async () => {
      toolbox = await setup();
      const version = await toolbox.provider.getRpcApiVersion();
      shouldSkip = version?.major == 0 && version?.minor < 20;
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
      const packagePath = __dirname + '/./data/serializer';
      packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);
    });

    it('Dev inspect transaction with PayAllSui', async () => {
      if (shouldSkip) {
        return;
      }
      const gasBudget = 1000;
      const coins =
        await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
          toolbox.address(),
          BigInt(DEFAULT_GAS_BUDGET)
        );

      const splitTxn = await signer.splitCoin({
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [2000, 2000, 2000],
        gasBudget: gasBudget,
        gasPayment: getObjectId(coins[1]),
      });
      const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
        getObjectId(c)
      );

      await validateDevInspectTransaction(signer, {
        kind: 'payAllSui',
        data: {
          inputCoins: splitCoins,
          recipient: DEFAULT_RECIPIENT,
          gasBudget: gasBudget,
        },
      });
    });

    it('Move Call that returns struct', async () => {
      if (shouldSkip) {
        return;
      }
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const moveCall = {
        packageObjectId: packageId,
        module: 'serializer_tests',
        function: 'return_struct',
        typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>'],
        arguments: [coins[0].objectId],
        gasBudget: DEFAULT_GAS_BUDGET,
      };

      await validateDevInspectTransaction(signer, {
        kind: 'moveCall',
        data: moveCall,
      });
    });

    it('Move Call that aborts', async () => {
      if (shouldSkip) {
        return;
      }
      const moveCall = {
        packageObjectId: packageId,
        module: 'serializer_tests',
        function: 'test_abort',
        typeArguments: [],
        arguments: [],
        gasBudget: DEFAULT_GAS_BUDGET,
      };

      await validateDevInspectTransaction(signer, {
        kind: 'moveCall',
        data: moveCall,
      });
    });
  }
);

async function validateDevInspectTransaction(
  signer: RawSigner,
  txn: SignableTransaction
) {
  const localDigest = await signer.getTransactionDigest(txn);
  const result = await signer.devInspectTransaction(txn);
  expect(localDigest).toEqual(result.effects.transactionDigest);
}
