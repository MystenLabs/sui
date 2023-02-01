// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  RawSigner,
  getExecutionStatusType,
  getCreatedObjects,
} from '../../src';
import { setup, TestToolbox } from './utils/setup';

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

  it('test getCommiteeInfo', async () => {
    const commiteeInfo = await toolbox.provider.getCommitteeInfo(0);
    expect(commiteeInfo.committee_info?.length).greaterThan(0);
  });

  it('test getSuiSystemState', async () => {
    await toolbox.provider.getSuiSystemState();
  });
});

async function addDelegation(signer: RawSigner) {
  const coins = await signer.provider.getGasObjectsOwnedByAddress(
    await signer.getAddress()
  );

  const validators = await signer.provider.getValidators();

  const tx = {
    coins: [coins[0].objectId],
    amount: DEFAULT_STAKED_AMOUNT,
    validator: validators[0].sui_address,
    gasBudget: 10000,
  };

  return await signer.signAndExecuteTransaction(
    await signer.serializer.serializeToBytes(
      await signer.getAddress(),
      {
        kind: 'requestAddDelegation',
        data: tx,
      },
      'Commit'
    )
  );
}
