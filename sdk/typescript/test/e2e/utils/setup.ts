// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import axios from 'axios';
import { expect } from 'vitest';
import {
  Base64DataBuffer,
  Ed25519Keypair,
  getEvents,
  getExecutionStatusType,
  JsonRpcProvider,
  JsonRpcProviderWithCache,
  LATEST_RPC_API_VERSION,
  ObjectId,
  RawSigner,
} from '../../../src';

const DEFAULT_FAUCET_URL = 'http://127.0.0.1:9123/faucet';
const DEFAULT_FULLNODE_URL = 'http://127.0.0.1:9000';

export const DEFAULT_RECIPIENT = '0x36096be6a0314052931babed39f53c0666a6b0df';
export const DEFAULT_RECIPIENT_2 = '0x46096be6a0314052931babed39f53c0666a6b0da';
export const DEFAULT_GAS_BUDGET = 10000;

export class TestToolbox {
  constructor(
    public keypair: Ed25519Keypair,
    public provider: JsonRpcProvider
  ) {}

  address() {
    return this.keypair.getPublicKey().toSuiAddress();
  }
}

export async function requestToken(recipient: string): Promise<void> {
  const res = await axios.post<{ ok: boolean }>(DEFAULT_FAUCET_URL, {
    recipient,
  });
  if (!res.data.ok) {
    throw new Error('Unable to invoke local faucet.');
  }
}

type ProviderType = 'rpc' | 'rpc-with-cache';

export function getProvider(providerType: ProviderType): JsonRpcProvider {
  return providerType === 'rpc'
    ? new JsonRpcProvider(DEFAULT_FULLNODE_URL, false, LATEST_RPC_API_VERSION)
    : new JsonRpcProviderWithCache(
        DEFAULT_FULLNODE_URL,
        false,
        LATEST_RPC_API_VERSION
      );
}

export async function setup(providerType: ProviderType = 'rpc') {
  const keypair = Ed25519Keypair.generate();
  const address = keypair.getPublicKey().toSuiAddress();
  await requestToken(address);

  return new TestToolbox(keypair, getProvider(providerType));
}

export async function publishPackage(
  signer: RawSigner,
  useLocalTxnBuilder: boolean,
  packagePath: string
): Promise<ObjectId> {
  const { execSync } = require('child_process');
  const compiledModules = JSON.parse(
    execSync(
      `cargo run --bin sui move build --dump-bytecode-as-base64 --path ${packagePath}`,
      { encoding: 'utf-8' }
    )
  );
  const publishTxn = await signer.publishWithRequestType({
    compiledModules: useLocalTxnBuilder
      ? compiledModules.map((m: any) =>
          Array.from(new Base64DataBuffer(m).getData())
        )
      : compiledModules,
    gasBudget: DEFAULT_GAS_BUDGET,
  });
  expect(getExecutionStatusType(publishTxn)).toEqual('success');

  const publishEvent = getEvents(publishTxn).filter(
    (e: any) => 'publish' in e
  )[0];
  return publishEvent.publish.packageId;
}
