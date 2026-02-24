// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiClient } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';

import { Pool } from '../../src/pool';
import { IncludeAdminCapStrategy } from '../../src/splitStrategies';
import {
  getKeyPair,
  sleep,
  totalBalance,
  mintNFTTxb,
} from '../helpers/helpers';
import { getEnvironmentVariables } from '../helpers/setupEnvironmentVariables';
import { SetupTestsHelper } from '../helpers/setupTestsHelper';

const env = getEnvironmentVariables('../test/.test.env', true);
const adminKeypair = getKeyPair(env.ADMIN_SECRET_KEY);
const client = new SuiClient({
  url: env.SUI_NODE,
});

describe('🌊 Basic flow of sign & execute tx block', () => {
  beforeEach(async () => {
    // Reset the mock before each test
    jest.clearAllMocks();
    const helper = new SetupTestsHelper();
    await helper.setupAdmin(2, 5);
    await sleep(5000);
  });

  it('checks truthy object ownership', async () => {
    // Create a pool
    const pool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const objects = pool.objects;

    // Check that pool was created and contains at least 1 object
    expect(objects.size).toBeGreaterThan(0);

    // Admin transfers an object that belongs to him back to himself.
    const txb = new TransactionBlock();
    const adminAddress = adminKeypair.getPublicKey().toSuiAddress();
    txb.transferObjects(
      [txb.object(env.NFT_APP_ADMIN_CAP)],
      txb.pure(adminAddress),
    );

    // Include a transfer coin transaction in the transaction block
    const [coin] = txb.splitCoins(txb.gas, [txb.pure(1)]);
    txb.transferObjects([coin], txb.pure(env.TEST_USER_ADDRESS)); // Transferring the object to a test address
    txb.setSender(adminAddress);
    // Check ownership of the objects in the transaction block.
    expect(pool.checkTotalOwnership(txb, client)).toBeTruthy();
  });

  const falsyObjectIds =
    '0x05d97725fd32745a35fe746489a92c80d0b7eac00vba2df51216457e5e9d8807'; // Random string
  it.each([falsyObjectIds])(
    'checks falsy object ownership',
    async (falsyObjectId) => {
      // Create a pool
      const pool: Pool = await Pool.full({
        keypair: adminKeypair,
        client: client,
      });
      const objects = pool.objects;

      // Check that pool was created and contains at least 1 object
      expect(objects.size).toBeGreaterThan(0);

      // Admin transfers a random object that doesn't belong to himself.
      const txb = new TransactionBlock();
      const adminAddress = adminKeypair.getPublicKey().toSuiAddress();
      txb.setSender(adminAddress);

      txb.transferObjects(
        [txb.object(falsyObjectId)],
        txb.pure(env.TEST_USER_ADDRESS),
      );

      // Check ownership of the objects in the transaction block.
      await expect(pool.checkTotalOwnership(txb, client)).rejects.toThrow();
    },
  );

  /*
  When a pool signs and executes a txb, it should use only its own coins for gas.
  */
  it("uses only the pool's coins for gas", async () => {
    const mainPool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });

    const poolTwo: Pool = await mainPool.split(
      client,
      new IncludeAdminCapStrategy(env.NFT_APP_PACKAGE_ID),
    );
    const mainPoolTotalBalanceBeforeTransaction = totalBalance(mainPool);
    /*
    Create a nft object using the first pool and
    transfer it to yourself (admin
    */
    const poolTwoBalanceBeforeTransaction = totalBalance(poolTwo);
    const txb = mintNFTTxb(env, adminKeypair);
    const res = await poolTwo.signAndExecuteTransactionBlock({
      client,
      transactionBlockLambda: txb,
      requestType: 'WaitForLocalExecution',
      options: {
        showEffects: true,
        showEvents: true,
        showObjectChanges: true,
      },
    });
    expect(res?.effects?.status.status).toEqual('success');
    const mainPoolTotalBalanceAfterTransaction = totalBalance(mainPool);
    const poolTwoBalanceAfterTransaction = totalBalance(poolTwo);
    if (!res?.effects?.gasUsed) {
      console.warn('Gas used by pool is undefined');
    }

    expect(mainPoolTotalBalanceBeforeTransaction).toEqual(
      mainPoolTotalBalanceAfterTransaction,
    );
    expect(poolTwoBalanceAfterTransaction).toBeLessThan(
      poolTwoBalanceBeforeTransaction,
    );
  });
});

describe('Transaction block execution directly from pool', () => {
  beforeEach(async () => {
    // Reset the mock before each test
    jest.clearAllMocks();
    const helper = new SetupTestsHelper();
    await helper.setupAdmin(0, 5);
    await sleep(2000);
  });

  it('mints nft and transfers it to self', async () => {
    // Create a main pool and split it to use a different pool for the
    // transaction execution
    const mainPool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const pool = await mainPool.split(
      client,
      new IncludeAdminCapStrategy(env.NFT_APP_PACKAGE_ID),
    );
    const objects = pool.objects;

    // Check that pool was created and contains at least 1 object
    expect(objects.size).toBeGreaterThan(0);

    const txb = mintNFTTxb(env, adminKeypair);
    const res = await pool.signAndExecuteTransactionBlock({
      client,
      transactionBlockLambda: txb,
      requestType: 'WaitForLocalExecution',
      options: {
        showEffects: true,
        showEvents: true,
        showObjectChanges: true,
      },
    });
    expect(res.effects?.status.status ?? '').toEqual('success');

    // Assert that the pool was updated by checking that the object
    // that was created is in the object's pool.
    const createdObj =
      res.effects && res.effects.created && res.effects.created[0]
        ? res.effects.created[0]
        : { reference: { objectId: '' } };

    expect(pool.objects.has(createdObj.reference.objectId)).toBeTruthy();
  });

  it('mints nft, transfers it to a test user', async () => {
    const mainPool: Pool = await Pool.full({
      keypair: adminKeypair,
      client: client,
    });
    const pool = await mainPool.split(
      client,
      new IncludeAdminCapStrategy(env.NFT_APP_PACKAGE_ID),
    );
    const objects = pool.objects;

    // Check that pool was created and contains at least 1 object
    expect(objects.size).toBeGreaterThan(0);

    // Admin transfers an object that belongs to him back to himself.
    const txb = mintNFTTxb(env, adminKeypair);
    const res = await pool.signAndExecuteTransactionBlock({
      client,
      transactionBlockLambda: txb,
      requestType: 'WaitForLocalExecution',
      options: {
        showEffects: true,
        showEvents: true,
        showObjectChanges: true,
      },
    });

    expect(res?.effects?.status.status).toEqual('success');
  });
});
