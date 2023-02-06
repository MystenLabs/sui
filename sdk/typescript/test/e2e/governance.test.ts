// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  RawSigner,
  getExecutionStatusType,
  SuiSystemStateUtil,
  LocalTxnDataSerializer,
  SUI_SYSTEM_STATE_OBJECT_ID,
  assert as superStructAssert,
  getMoveObject,
  MoveSuiSystemObjectFields,
} from '../../src';
import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

const DEFAULT_STAKED_AMOUNT = 1;

describe.each([{ useLocalTxnBuilder: true }, { useLocalTxnBuilder: false }])(
  'Governance API',
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
          : undefined,
      );
    });

    it('test requestAddDelegation', async () => {
      const result = await addDelegation(signer);
      expect(getExecutionStatusType(result)).toEqual('success');
    });

    it('test getDelegatedStakes', async () => {
      const stakes = await toolbox.provider.getDelegatedStakes(
        toolbox.address(),
      );
      expect(stakes.length).greaterThan(0);
    });

    it('test requestWithdrawDelegation', async () => {
      // TODO: implement this
    });

    it('test requestSwitchDelegation', async () => {
      // TODO: implement this
    });

    it('test getValidators', async () => {
      const validators = await toolbox.provider.getValidators();
      expect(validators.length).greaterThan(0);
    });

    it('test getCommiteeInfo', async () => {
      const commiteeInfo = await toolbox.provider.getCommitteeInfo(0);
      expect(commiteeInfo.committee_info?.length).greaterThan(0);
    });

    it('test getSuiSystemState', async () => {
      await toolbox.provider.getSuiSystemState();
    });

    it('test Validator definition', async () => {
      const data = await toolbox.provider.getObject(SUI_SYSTEM_STATE_OBJECT_ID);
      const moveObject = getMoveObject(data);
      superStructAssert(moveObject!.fields, MoveSuiSystemObjectFields);
    });
  },
);

async function addDelegation(signer: RawSigner) {
  const coins = await signer.provider.getGasObjectsOwnedByAddress(
    await signer.getAddress(),
  );

  const validators = await signer.provider.getValidators();

  return await signer.signAndExecuteTransaction(
    await SuiSystemStateUtil.newRequestAddDelegationTxn(
      [coins[0].objectId],
      BigInt(DEFAULT_STAKED_AMOUNT),
      validators[0].sui_address,
      DEFAULT_GAS_BUDGET,
    ),
  );
}
