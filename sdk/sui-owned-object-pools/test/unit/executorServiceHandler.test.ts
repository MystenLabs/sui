// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { SuiTransactionBlockResponse } from '@mysten/sui.js/client';
import { SuiClient } from '@mysten/sui.js/client';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import type { SignatureWithBytes } from '@mysten/sui.js/cryptography';
import { ExecutorServiceHandler } from '../../src/executorServiceHandler';
import { Pool } from '../../src/pool';
import { getKeyPair, mintNFTTxb, sleep } from '../helpers/helpers';
import { getEnvironmentVariables } from '../helpers/setupEnvironmentVariables';
import { SetupTestsHelper } from '../helpers/setupTestsHelper';
import {
  IncludeAdminCapStrategy,
  SponsoredAdminCapStrategy,
} from '../../src/splitStrategies';
import { TransactionBlockWithLambda } from '../../src/transactions';

const env = getEnvironmentVariables('../test/.test.env', true);
const adminKeypair = getKeyPair(env.ADMIN_SECRET_KEY);
const client = new SuiClient({
  url: env.SUI_NODE,
});
const MIST_TO_TRANSFER = 10;
const NUMBER_OF_TRANSACTION_TO_EXECUTE = 15;
const COINS_NEEDED = NUMBER_OF_TRANSACTION_TO_EXECUTE * 2;
const helper = new SetupTestsHelper();

// Create a transaction that transfers MIST from the admin to a test user address.
function createPaymentTxb(recipient: string): TransactionBlock {
  const txb = new TransactionBlock();
  const [coin] = txb.splitCoins(txb.gas, [txb.pure(MIST_TO_TRANSFER)]);
  txb.transferObjects([coin], txb.pure(recipient));
  return txb;
}

describe('Execute multiple transactions with ExecutorServiceHandler', () => {
  xit('smashes the coins into one', async () => {
    await helper.smashCoins(COINS_NEEDED);
  });

  it('parses coins from owned objects', async () => {
    const pool = await Pool.full({ client, keypair: adminKeypair });
    const coinsFromClient = new Map();
    let coins_resp;
    let cursor = null;
    do {
      coins_resp = await client.getAllCoins({
        owner: adminKeypair.toSuiAddress(),
        cursor,
      });
      coins_resp.data.forEach((coin) => {
        coinsFromClient.set(coin.coinObjectId, coin);
      });
      cursor = coins_resp?.nextCursor;
    } while (coins_resp.hasNextPage);
    const coinsFromOwnedObjects = pool.gasCoins;
    expect(
      Array.from(coinsFromOwnedObjects.keys()).every((key) => {
        return coinsFromClient.has(key);
      }),
    ).toBeTruthy();
  });

  it('executes multiple coin transfer (payment) transactions - case 1', async () => {
    await helper.setupAdmin(0, COINS_NEEDED);
    console.log(
      'Admin setup complete 🚀 - waiting for 5 seconds for effects to take place...',
    );
    await sleep(5000);
    // Pass this transaction to the ExecutorServiceHandler. The ExecutorServiceHandler will
    // forward the transaction to a worker pool, which will sign and execute the transaction.
    const eshandler = await ExecutorServiceHandler.initialize(
      adminKeypair,
      client,
      env.GET_WORKER_TIMEOUT_MS,
    );

    const promises: Promise<SuiTransactionBlockResponse>[] = [];
    let txb: TransactionBlockWithLambda;
    for (let i = 0; i < NUMBER_OF_TRANSACTION_TO_EXECUTE; i++) {
      txb = new TransactionBlockWithLambda(() =>
        createPaymentTxb(env.TEST_USER_ADDRESS),
      );
      promises.push(eshandler.execute(txb, client));
    }

    const results = await Promise.allSettled(promises);
    results.forEach((result) => {
      if (result.status === 'rejected') {
        console.error(result.reason);
      }
      expect(result.status).toEqual('fulfilled');
    });
  });

  it('executes multiple mint nft transactions using admin caps - case 2', async () => {
    const eshandler = await ExecutorServiceHandler.initialize(
      adminKeypair,
      client,
      env.GET_WORKER_TIMEOUT_MS,
    );
    const promises: Promise<SuiTransactionBlockResponse>[] = [];
    let txb: TransactionBlockWithLambda;
    for (let i = 0; i < NUMBER_OF_TRANSACTION_TO_EXECUTE; i++) {
      txb = mintNFTTxb(env, adminKeypair);
      promises.push(
        eshandler.execute(
          txb,
          client,
          new IncludeAdminCapStrategy(env.NFT_APP_PACKAGE_ID),
        ),
      );
    }

    const results = await Promise.allSettled(promises);
    results.forEach((result) => {
      if (result.status === 'rejected') {
        console.error(result.reason);
      }
      expect(result.status).toEqual('fulfilled');
    });
  });

  it('sponsors one transaction - case 3', async () => {
    /*
    Create a transaction that transfers MIST from the admin to a user address.
    The test user sponsors the transaction (i.e., pays the gas for it).
     */
    const eshandler = await ExecutorServiceHandler.initialize(
      adminKeypair,
      client,
      env.GET_WORKER_TIMEOUT_MS,
    );

    const sponsorLambda = async (
      txb: TransactionBlock,
    ): Promise<[SignatureWithBytes, SignatureWithBytes]> => {
      const kindBytes = await txb.build({
        client: client,
        onlyTransactionKind: true,
      });
      const tx = TransactionBlock.fromKind(kindBytes);
      tx.setSender(env.ADMIN_ADDRESS);
      tx.setGasOwner(env.TEST_USER_ADDRESS);
      let sponsorKeypair = getKeyPair(env.TEST_USER_SECRET);
      let sponsoredTx = await sponsorKeypair.signTransactionBlock(
        await tx.build({ client: client }),
      );
      const senderKeypair = getKeyPair(env.ADMIN_SECRET_KEY);
      let signedTX = await senderKeypair.signTransactionBlock(
        await TransactionBlock.from(sponsoredTx.bytes).build({
          client: client,
        }),
      );
      return [signedTX, sponsoredTx];
    };

    const promises: Promise<SuiTransactionBlockResponse>[] = [];
    let txb: TransactionBlockWithLambda;

    txb = mintNFTTxb(env, adminKeypair);
    promises.push(
      eshandler.execute(
        txb,
        client,
        new SponsoredAdminCapStrategy(env.NFT_APP_PACKAGE_ID),
        undefined,
        undefined,
        sponsorLambda,
      ),
    );

    const results = await Promise.allSettled(promises);
    results.forEach((result) => {
      if (result.status === 'rejected') {
        console.error(result.reason);
      }
      expect(result.status).toEqual('fulfilled');
    });
  });
});
