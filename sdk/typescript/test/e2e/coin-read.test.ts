// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  getObjectId,
  LocalTxnDataSerializer,
  normalizeSuiObjectId,
  ObjectId,
  RawSigner,
  SuiObjectInfo,
  SUI_TYPE_ARG,
} from '../../src';

import { publishPackage, setup, TestToolbox } from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }])(
    'CoinRead API',
    ({ useLocalTxnBuilder }) => {
      let toolbox: TestToolbox;
      let signer: RawSigner;
      let packageId: string;
      let shouldSkip: boolean;
  
      beforeAll(async () => {
        toolbox = await setup();
        const version = await toolbox.provider.getRpcApiVersion();
        shouldSkip = version?.major == 0 && version?.minor < 20;
        signer = new RawSigner(
          toolbox.keypair,
          toolbox.provider,
          useLocalTxnBuilder
            ? new LocalTxnDataSerializer(toolbox.provider)
            : undefined
        );
        const packagePath = __dirname + '/./data/coin_read';
        packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);
      });
    
      it("Get coins with/without type", async () => {
        const coins = await toolbox.provider.getObjectsOwnedByAddress(toolbox.address())
        const suiCoins = coins.filter((c) => Coin.isSUI(c));
        const testCoins = coins.filter((c) => Coin.getCoinTypeArg(c) == testType);
        const  coinsWithoutType = await toolbox.provider.getCoins(toolbox.address());
        const coinsWithType = await toolbox.provider.getCoins(toolbox.address(), testType);
        expect(coinsWithoutType).toStrictEqual(suiCoins);
      });
    
    }
)