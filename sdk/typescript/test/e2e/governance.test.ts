// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  RawSigner,
  getExecutionStatusType,
  SuiSystemStateUtil,
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

  it('test getValidators', async () => {
    const validators = await toolbox.provider.getValidators();
    expect(validators.length).greaterThan(0);
  });

  it('test getCommitteeInfo', async () => {
    const committeeInfo = await toolbox.provider.getCommitteeInfo(0);
    expect(committeeInfo.validators?.length).greaterThan(0);
  });

  it('test getSuiSystemState', async () => {
    await toolbox.provider.getSuiSystemState();
  });

  it('test getLatestSuiSystemState', async () => {
    await toolbox.provider.getLatestSuiSystemState();
  });
});

async function addDelegation(signer: RawSigner) {
  const coins = await signer.provider.getGasObjectsOwnedByAddress(
    await signer.getAddress(),
  );

  const validators = await signer.provider.getValidators();

  const tx = await SuiSystemStateUtil.newRequestAddDelegationTxn(
    signer.provider,
    [coins[0].objectId],
    BigInt(DEFAULT_STAKED_AMOUNT),
    validators[0].sui_address,
  );

  tx.setGasBudget(DEFAULT_GAS_BUDGET);

  return await signer.signAndExecuteTransaction(tx);
}
