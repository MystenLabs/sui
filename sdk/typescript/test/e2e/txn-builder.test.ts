// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getExecutionStatusType,
  getNewlyCreatedCoinRefsAfterSplit,
  getObjectId,
  LocalTxnDataSerializer,
  RawSigner,
} from '../../src';
import {
  DEFAULT_RECIPIENT,
  DEFAULT_GAS_BUDGET,
  SUI_SYSTEM_STATE_OBJECT_ID,
  setup,
  TestToolbox,
  DEFAULT_RECIPIENT_2,
} from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }, { useLocalTxnBuilder: false }])(
  'Transaction Builders',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;

    beforeAll(async () => {
      toolbox = await setup();
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
    });

    it('Split coin', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const txn = await signer.splitCoinWithRequestType({
        coinObjectId: coins[0].objectId,
        splitAmounts: [DEFAULT_GAS_BUDGET * 2],
        gasBudget: DEFAULT_GAS_BUDGET,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Merge coin', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const txn = await signer.mergeCoinWithRequestType({
        primaryCoin: coins[0].objectId,
        coinToMerge: coins[1].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Move Call', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const txn = await signer.executeMoveCallWithRequestType({
        packageObjectId: '0x2',
        module: 'devnet_nft',
        function: 'mint',
        typeArguments: [],
        arguments: [
          'Example NFT',
          'An NFT created by the wallet Command Line Tool',
          'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
        ],
        gasBudget: DEFAULT_GAS_BUDGET,
        gasPayment: coins[0].objectId,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Move Shared Object Call', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );

      const validators = await toolbox.getActiveValidators();
      const validator_metadata = (validators[0] as SuiMoveObject).fields
        .metadata;
      const validator_address = (validator_metadata as SuiMoveObject).fields
        .sui_address;

      const txn = await signer.executeMoveCallWithRequestType({
        packageObjectId: '0x2',
        module: 'sui_system',
        function: 'request_add_delegation',
        typeArguments: [],
        arguments: [
          SUI_SYSTEM_STATE_OBJECT_ID,
          coins[2].objectId,
          validator_address,
        ],
        gasBudget: DEFAULT_GAS_BUDGET,
        gasPayment: coins[3].objectId,
      });

      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Transfer Sui', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const txn = await signer.transferSuiWithRequestType({
        suiObjectId: coins[0].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipient: DEFAULT_RECIPIENT,
        amount: 100,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Transfer Object', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const txn = await signer.transferObjectWithRequestType({
        objectId: coins[0].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipient: DEFAULT_RECIPIENT,
        gasPayment: coins[1].objectId,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });

    it('Pay', async () => {
      const coins =
        await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
          toolbox.address(),
          BigInt(DEFAULT_GAS_BUDGET)
        );

      // get some new coins with small amount
      const splitTxn = await signer.splitCoinWithRequestType({
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [1, 2, 3],
        gasBudget: DEFAULT_GAS_BUDGET,
        gasPayment: getObjectId(coins[1]),
      });
      const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
        getObjectId(c)
      );

      // use the newly created coins as the input coins for the pay transaction
      const txn = await signer.payWithRequestType({
        inputCoins: splitCoins,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipients: [DEFAULT_RECIPIENT, DEFAULT_RECIPIENT_2],
        amounts: [4, 2],
        gasPayment: getObjectId(coins[2]),
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });
  }
);
