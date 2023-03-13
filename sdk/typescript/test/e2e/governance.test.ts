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

  it('test requestAddStake', async () => {
    const result = await addStake(signer);
    expect(getExecutionStatusType(result)).toEqual('success');
  });

  it('test getDelegatedStakes', async () => {
    const stakes = await toolbox.provider.getDelegatedStakes(toolbox.address());
    expect(stakes.length).greaterThan(0);
  });

  it('test requestWithdrawStake', async () => {
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

async function addStake(signer: RawSigner) {
  const coins = await signer.provider.getCoins({
    owner: await signer.getAddress(),
    coinType: SUI_TYPE_ARG,
  });

  const system = await signer.provider.getLatestSuiSystemState();
  const validators = system.activeValidators;

  const tx = await SuiSystemStateUtil.newRequestAddStakeTxn(
    signer.provider,
    [coins.data[0].coinObjectId],
    BigInt(DEFAULT_STAKED_AMOUNT),
    validators[0].suiAddress,
  );

  tx.setGasBudget(DEFAULT_GAS_BUDGET);

  return await signer.signAndExecuteTransaction(tx, {
    showEffects: true,
  });
}
