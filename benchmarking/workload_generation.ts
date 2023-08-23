// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Ed25519Keypair,
  getExecutionStatusType,
  getTransactionDigest,
  JsonRpcProvider,
  localnetConnection,
  RawSigner,
  TransactionBlock,
} from "../sdk/typescript/src";

const SHARDS = 16;
const USERS_PER_SHARD = 2;
const TOTAL_USERS = SHARDS * USERS_PER_SHARD;
const TXS_PER_SHARD = 1000;
const SIMPLE_TX_PROB = 1.0;
const HEAVY_TX_SPLITS = 100;
const CROSS_SHARD_FREQ = 0.01;

type User = {
  keypair: Ed25519Keypair;
  address: string;
  signer: RawSigner;
};

let provider: JsonRpcProvider;
let users: User[] = [];

function assert(condition: unknown, msg?: string): asserts condition {
  if (condition === false) throw new Error(msg);
}

async function setup() {
  provider = new JsonRpcProvider(localnetConnection);
  for (let i = 0; i < TOTAL_USERS; i++) {
    const keypair = new Ed25519Keypair();
    const user = {
      keypair,
      address: keypair.getPublicKey().toSuiAddress(),
      signer: new RawSigner(keypair, provider),
    };
    await provider.requestSuiFromFaucet(user.address);
    users.push(user);
  }
}

async function generateWorkload() {
  for (let i = 0; i < TXS_PER_SHARD; i++) {
    const results: Promise<void>[] = [];

    for (let shard = 0; shard < SHARDS; shard++) {
      const userIdx = USERS_PER_SHARD * shard +
        Math.floor(Math.random() * USERS_PER_SHARD);
      let otherUserIdx = userIdx;
      while (otherUserIdx === userIdx) {
        otherUserIdx = USERS_PER_SHARD * shard +
          Math.floor(Math.random() * USERS_PER_SHARD);
      }
      const user = users[userIdx];
      const otherUser = users[otherUserIdx];
      const tx = new TransactionBlock();
      if (Math.random() >= SIMPLE_TX_PROB) {
        for (let j = 0; j < HEAVY_TX_SPLITS; j++) {
          const [coin] = tx.splitCoins(tx.gas, [tx.pure(1000)]);
          tx.mergeCoins(tx.gas, [coin]);
        }
      }
      const [coin] = tx.splitCoins(tx.gas, [tx.pure(1000)]);
      tx.transferObjects([coin], tx.pure(otherUser.address));
      results.push(validateTransaction(user.signer, tx));
    }

    await Promise.all(results);

    if (i % 10 === 0) {
      console.log(
        "\r%d/%d txs (%f\%)         ",
        i * SHARDS,
        TXS_PER_SHARD * SHARDS,
        Math.floor(i * 10_000.0 / TXS_PER_SHARD) / 100.0,
      );
    }
  }
}

async function validateTransaction(signer: RawSigner, tx: TransactionBlock) {
  const localDigest = await signer.getTransactionBlockDigest(tx);
  const result = await signer.signAndExecuteTransactionBlock({
    transactionBlock: tx,
    options: {
      showEffects: true,
    },
  });
  assert(localDigest === getTransactionDigest(result));
  assert(getExecutionStatusType(result) === "success");
}

await setup();
await generateWorkload();
