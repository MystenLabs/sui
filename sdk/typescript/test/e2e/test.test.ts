// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, beforeAll } from 'vitest';
import {
  LocalTxnDataSerializer,
  PureArg,
  RawSigner,
  SUI_TYPE_ARG,
} from '../../src';
import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

describe('Test Pure Arg', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
  });

  it('Move Call with Pure Args', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
    await signer.signAndExecuteTransaction({
        kind: 'moveCall',
        data: {
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
        },
      })
      const pureArg = new PureArg([
        11,  69, 120, 97, 109,
        112, 108, 101, 32,  78,
        70,  84
      ]);
      await signer.signAndExecuteTransaction({
        kind: 'moveCall',
        data: {
          packageObjectId: '0x2',
          module: 'devnet_nft',
          function: 'mint',
          typeArguments: [],
          arguments: [
            pureArg,
            'An NFT created by the wallet Command Line Tool',
            'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
          ],
          gasBudget: DEFAULT_GAS_BUDGET,
          gasPayment: coins[1].objectId,
        },
      })
  
      const objects = (await toolbox.provider.getObjectsOwnedByAddress(toolbox.address())).filter((o) => {
        o.type != SUI_TYPE_ARG
      });
      const ob1 = toolbox.provider.getObject(objects[0].objectId);
      const ob2 = toolbox.provider.getObject(objects[1].objectId);
      console.log(ob1);
      console.log(ob2);
  });
});
