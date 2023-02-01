// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, expectTypeOf } from 'vitest';
import {
  RawSigner,
  DelegatedStake,
  ObjectId,
  normalizeSuiObjectId,
  getExecutionStatusType,
} from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Governance API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
  });

  it('test requestAddDelegation', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );

    const tx = {
      coins: [coins[0].objectId],
      amount: '1',
      validator: normalizeSuiObjectId('0x1'),
      gasBudget: 10000,
    };

    const result = await signer.signAndExecuteTransaction(
      await signer.serializer.serializeToBytes(
        await signer.getAddress(),
        {
          kind: 'requestAddDelegation',
          data: tx,
        },
        'Commit'
      )
    );
    expect(getExecutionStatusType(result)).toEqual('success');
  });
  it('test getDelegatedStakes', async () => {
    const stakes = await toolbox.provider.getDelegatedStakes(toolbox.address());
    // TODO: not able to test this, needs address with stake
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
    const systemState = await toolbox.provider.getSuiSystemState();
    expect(systemState.epoch).greaterThan(0);
  });
});
