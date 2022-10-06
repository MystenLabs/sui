// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import axios from 'axios';
import {
  Ed25519Keypair,
  JsonRpcProvider,
  JsonRpcProviderWithCache,
} from '../../../src';

const DEFAULT_FAUCET_URL = 'http://127.0.0.1:9123/faucet';
const DEFAULT_FULLNODE_URL = 'http://127.0.0.1:9000';
const DEFAULT_GATEWAY_URL = 'http://127.0.0.1:5001';

export const DEFAULT_RECIPIENT = '0x36096be6a0314052931babed39f53c0666a6b0df';
export const DEFAULT_GAS_BUDGET = 1000;

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

type RPCType = 'gateway' | 'fullnode';
type ProviderType = 'rpc' | 'rpc-with-cache';

export function getProvider(
  rpcType: RPCType,
  providerType: ProviderType
): JsonRpcProvider {
  const url =
    rpcType === 'fullnode' ? DEFAULT_FULLNODE_URL : DEFAULT_GATEWAY_URL;
  return providerType === 'rpc'
    ? new JsonRpcProvider(url, false)
    : new JsonRpcProviderWithCache(url, false);
}

export async function setup(
  rpcType: RPCType = 'fullnode',
  providerType: ProviderType = 'rpc'
) {
  const keypair = Ed25519Keypair.generate();
  const address = keypair.getPublicKey().toSuiAddress();
  await requestToken(address);

  return new TestToolbox(keypair, getProvider(rpcType, providerType));
}
