// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiClient } from '@mysten/sui.js/client';

import { Pool } from '../../src/pool';
import type { SplitStrategy } from '../../src/splitStrategies';
import { getKeyPair, sleep, totalBalance } from '../helpers/helpers';
import { getEnvironmentVariables } from '../helpers/setupEnvironmentVariables';
import { SetupTestsHelper } from '../helpers/setupTestsHelper';
import { DefaultSplitStrategy } from '../../src/splitStrategies';

const env = getEnvironmentVariables('../test/.test.env', true);
const adminKeypair = getKeyPair(env.ADMIN_SECRET_KEY);
const client = new SuiClient({
  url: env.SUI_NODE,
});

const MINIMUM_NUMBER_OF_ADMIN_OBJECTS = 3;
const helper = new SetupTestsHelper();

describe('Pool creation with factory', () => {
  beforeEach(() => {
    // Reset the mock before each test`
    jest.clearAllMocks();
  });

  /// WARNING this test might fail if the account
  /// has no coins or objects (NFTs).
  it('creates a pool correctly', async () => {
    const pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });

    expect(pool.objects.size).toBeGreaterThan(0);
  });
});

describe('✂️ Pool splitting', () => {
  beforeEach(async () => {
    await helper.setupAdmin(
      MINIMUM_NUMBER_OF_ADMIN_OBJECTS,
      MINIMUM_NUMBER_OF_ADMIN_OBJECTS * 2,
    );
    await sleep(2000);
  });

  it('should throw error: splits a pool not moving anything to new pool using always-false predicate', async () => {
    const initial_pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const num_objects_before_split = initial_pool.objects.size;
    const splitStrategy: SplitStrategy = {
      // eslint-disable-next-line no-unused-vars
      pred: (_: any | undefined) => false,
      succeeded: () => true,
    };
    const new_pool: Pool = await initial_pool.split(client, splitStrategy);
    const num_objects_new_pool = new_pool.objects.size;

    const num_objects_after_split = initial_pool.objects.size;
    expect(num_objects_new_pool).toEqual(0);
    expect(num_objects_before_split).toEqual(num_objects_after_split);
  });

  it('should throw error: splits a pool not moving anything to the new pool by using always-null predicate', async () => {
    const initial_pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const num_objects_before_split = initial_pool.objects.size;
    const splitStrategy: SplitStrategy = {
      // eslint-disable-next-line no-unused-vars
      pred: (_: any | undefined) => null,
      succeeded: () => true,
    };
    const new_pool: Pool = await initial_pool.split(client, splitStrategy);
    const num_objects_new_pool = new_pool.objects.size;
    const num_objects_after_split = initial_pool.objects.size;

    expect(num_objects_new_pool).toEqual(0);
    expect(num_objects_before_split).toEqual(num_objects_after_split);
  });

  it('splits a pool using the default predicate', async () => {
    // Create a pool
    const initial_pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const new_pool: Pool = await initial_pool.split(client);

    const num_objects_new_pool = new_pool.objects.size;
    expect(num_objects_new_pool).toBeGreaterThanOrEqual(1);

    const newPoolBalance = totalBalance(new_pool);
    expect(newPoolBalance).toBeGreaterThanOrEqual(
      DefaultSplitStrategy.defaultMinimumBalance,
    );
  });

  it('merges two pools', async () => {
    // Create the pool
    const initial_pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const pool1: Pool = await initial_pool.split(client);
    const pool1ObjectsBeforeMerge = Array.from(pool1.objects.keys());

    const pool2: Pool = await initial_pool.split(client);
    const pool2ObjectsBeforeMerge = Array.from(pool2.objects.keys());

    pool1.merge(pool2);

    expect(
      pool1ObjectsBeforeMerge.every((o) => pool1.objects.has(o)),
    ).toBeTruthy();

    expect(
      pool2ObjectsBeforeMerge.every((o) => pool1.objects.has(o)),
    ).toBeTruthy();

    expect(pool2.objects.size).toEqual(0);
  });

  it('checks that no pool contains the same objects after split', async () => {
    const initial_pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const NUMBER_OF_NEW_POOLS = 4;
    let newPool: Pool;
    const keysSet = new Set<string>();
    let totalSize = 0;

    for (let i = 0; i < NUMBER_OF_NEW_POOLS; i++) {
      newPool = await initial_pool.split(client);
      Array.from(newPool.objects.keys()).forEach((key) => {
        keysSet.add(key);
      });
      totalSize += newPool.objects.size;
    }
    // If the keySet size is smaller than the total size, it means that there are
    // some duplicate keys in the pools, meaning that there are some objects
    // present in 2 (or more) pools. Which would be wrong.
    expect(keysSet.size).toEqual(totalSize);
  });
});
