// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  RawSigner,
  getExecutionStatusType,
  SuiSystemStateUtil,
  SUI_TYPE_ARG,
} from '../../src';
import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

const DEFAULT_STAKED_AMOUNT = 1;

describe('Governance API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
  });

  it('test requestAddDelegation', async () => {
    const result = await addDelegation(signer);
    expect(getExecutionStatusType(result)).toEqual('success');
  });

  it('test getDelegatedStakes', async () => {
    const stakes = await toolbox.provider.getDelegatedStakes(toolbox.address());
    expect(stakes.length).greaterThan(0);
  });

  it('test requestWithdrawDelegation', async () => {
    // TODO: implement this
  });

  it('test requestSwitchDelegation', async () => {
    // TODO: implement this
  });

  it('test getCommitteeInfo', async () => {
    const committeeInfo = await toolbox.provider.getCommitteeInfo(0);
    expect(committeeInfo.validators?.length).greaterThan(0);
  });

  it('test getLatestSuiSystemState', async () => {
    await toolbox.provider.getLatestSuiSystemState();
  });
});

async function addDelegation(signer: RawSigner) {
  const coins = await signer.provider.getCoins(
    await signer.getAddress(),
    SUI_TYPE_ARG,
    null,
    null,
  );

  const system = await signer.provider.getLatestSuiSystemState();
  const validators = system.active_validators;

  const tx = await SuiSystemStateUtil.newRequestAddDelegationTxn(
    signer.provider,
    [coins.data[0].coinObjectId],
    BigInt(DEFAULT_STAKED_AMOUNT),
    validators[0].sui_address,
  );

  tx.setGasBudget(DEFAULT_GAS_BUDGET);

  return await signer.signAndExecuteTransaction(tx);
}
