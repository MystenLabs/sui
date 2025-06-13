// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { SuiClient } from '@mysten/sui.js/client';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { fromB64 } from '@mysten/sui.js/utils';
import type { Pool } from '../../src/pool';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import type { EnvironmentVariables } from './setupEnvironmentVariables';
import { TransactionBlockWithLambda } from '../../src/transactions';

/**
 * Returns an Ed25519Keypair object generated from the given private key.
 * @param privateKey - The private key to generate the keypair from.
 * @returns The Ed25519Keypair object generated from the given private key.
 */
export function getKeyPair(privateKey: string): Ed25519Keypair {
  const privateKeyArray = Array.from(fromB64(privateKey));
  privateKeyArray.shift();
  return Ed25519Keypair.fromSecretKey(Uint8Array.from(privateKeyArray));
}

/**
 * Asynchronously waits for the specified amount of time.
 * @param ms - The number of milliseconds to wait.
 * @returns A promise that resolves after the specified time has elapsed.
 */
export async function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function getAllCoinsFromClient(client: SuiClient, owner: string) {
  const coinsFromClient = new Map();
  let coins_resp;
  let cursor = null;
  do {
    coins_resp = await client.getAllCoins({
      owner,
      cursor,
    });
    coins_resp.data.forEach((coin) => {
      coinsFromClient.set(coin.coinObjectId, coin);
    });
    cursor = coins_resp?.nextCursor;
  } while (coins_resp.hasNextPage);
  return coinsFromClient;
}

/**
 * Calculates the total balance of a pool.
 * @param pool - The pool to calculate the balance of.
 */
export function totalBalance(pool: Pool) {
  let balance = 0;
  pool.gasCoins.forEach((c) => {
    balance += c.balance ?? 0;
  });
  return balance;
}

export function mintNFTTxb(
  env: EnvironmentVariables,
  adminKeypair: Ed25519Keypair,
): TransactionBlockWithLambda {
  const txbLambda = (adminCapId: string) => {
    const txb = new TransactionBlock();
    const hero = txb.moveCall({
      arguments: [
        txb.object(adminCapId),
        txb.pure('zed'),
        txb.pure('gold'),
        txb.pure(3),
        txb.pure('ipfs://example.com/'),
      ],
      target: `${env.NFT_APP_PACKAGE_ID}::hero_nft::mint_hero`,
    });

    txb.transferObjects(
      [hero],
      txb.pure(adminKeypair.getPublicKey().toSuiAddress()),
    );
    txb.setGasBudget(10000000);
    return txb;
  };
  return new TransactionBlockWithLambda(txbLambda, ['AdminCap']);
}
