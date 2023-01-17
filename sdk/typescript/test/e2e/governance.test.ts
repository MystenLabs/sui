// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, expectTypeOf } from 'vitest';
import {
  LocalTxnDataSerializer,
  RawSigner,
  SuiSystemState,
  DelegatedStake,
} from '../../src';
import {
  setup,
  TestToolbox,
} from './utils/setup';

describe('Governance API', () => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    // let packageId: string;
    // let shouldSkip: boolean;

    beforeAll(async () => {
      toolbox = await setup();
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        new LocalTxnDataSerializer(toolbox.provider)
      );
    });

    it('test getDelegatedStakes', async () => {
      const stakes = await toolbox.provider.getDelegatedStakes(
        toolbox.address(),
      );
      expectTypeOf(stakes).toBeArray(DelegatedStake)
      // console.log(stakes);
    });
    it('test getValidators', async () => {
      const validators = await toolbox.provider.getValidators();
      expect(validators.length).greaterThan(0);

    });
    it('test getCommiteeInfo', async () => {
      const commiteeInfo = await toolbox.provider.getCommitteeInfo(0);
      expect(commiteeInfo.committee_info?.length).greaterThan(0);

      const commiteeInfo2 = await toolbox.provider.getCommitteeInfo(100);
      expect(commiteeInfo2.committee_info).to.toEqual(null);

    });
    it('test getSuiSystemState', async () => {
      const systemState = await toolbox.provider.getSuiSystemState();
      expectTypeOf(systemState).toBeAny(SuiSystemState)
    }); 
    
  }
);
