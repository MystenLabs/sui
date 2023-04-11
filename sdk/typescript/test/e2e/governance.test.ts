// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  RawSigner,
  getExecutionStatusType,
  SuiSystemStateUtil,
  SUI_TYPE_ARG,
} from '../../src';
import { setup, TestToolbox } from './utils/setup';

const DEFAULT_STAKE_AMOUNT = 1000000000;

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
    await addStake(signer);
    const stakes = await toolbox.provider.getStakes({
      owner: toolbox.address(),
    });
    const stakesById = await toolbox.provider.getStakesByIds({
      stakedSuiIds: [stakes[0].stakes[0].stakedSuiId],
    });
    expect(stakes.length).greaterThan(0);
    expect(stakesById[0].stakes[0]).toEqual(stakes[0].stakes[0]);
  });

  it('test requestWithdrawStake', async () => {
    // TODO: implement this
  });

  it('test getCommitteeInfo', async () => {
    const committeeInfo = await toolbox.provider.getCommitteeInfo({
      epoch: '0',
    });
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
    BigInt(DEFAULT_STAKE_AMOUNT),
    validators[0].suiAddress,
  );

  return await signer.signAndExecuteTransactionBlock({
    transactionBlock: tx,
    options: {
      showEffects: true,
    },
  });
}
