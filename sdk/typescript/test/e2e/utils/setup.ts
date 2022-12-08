// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect } from 'vitest';
import {
  Base64DataBuffer,
  Ed25519Keypair,
  getEvents,
  getExecutionStatusType,
  JsonRpcProvider,
  JsonRpcProviderWithCache,
  Network,
  NETWORK_TO_API,
  ObjectId,
  RawSigner,
} from '../../../src';
import { retry } from 'ts-retry-promise';
import { FaucetRateLimitError } from '../../../src/rpc/faucet-client';

const TEST_ENDPOINTS = NETWORK_TO_API[Network.LOCAL];
const DEFAULT_FAUCET_URL =
  import.meta.env.VITE_FAUCET_URL ?? TEST_ENDPOINTS.faucet;
const DEFAULT_FULLNODE_URL =
  import.meta.env.VITE_FULLNODE_URL ?? TEST_ENDPOINTS.fullNode;

export const DEFAULT_RECIPIENT = '0x36096be6a0314052931babed39f53c0666a6b0df';
export const DEFAULT_RECIPIENT_2 = '0x46096be6a0314052931babed39f53c0666a6b0da';
export const DEFAULT_GAS_BUDGET = 10000;

export const SUI_SYSTEM_STATE_OBJECT_ID =
  '0x0000000000000000000000000000000000000005';

export class TestToolbox {
  constructor(
    public keypair: Ed25519Keypair,
    public provider: JsonRpcProvider
  ) {}

  address() {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  public async getActiveValidators(): Promise<Array<SuiMoveObject>> {
    const contents = await this.provider.getObject(SUI_SYSTEM_STATE_OBJECT_ID);
    const data = (contents.details as SuiObject).data;
    const validators = (data as SuiMoveObject).fields.validators;
    const active_validators = (validators as SuiMoveObject).fields
      .active_validators;
    return active_validators as Array<SuiMoveObject>;
  }
}

type ProviderType = 'rpc' | 'rpc-with-cache';

export function getProvider(providerType: ProviderType): JsonRpcProvider {
  return providerType === 'rpc'
    ? new JsonRpcProvider(DEFAULT_FULLNODE_URL, {
        skipDataValidation: false,
        faucetURL: DEFAULT_FAUCET_URL,
      })
    : new JsonRpcProviderWithCache(DEFAULT_FULLNODE_URL, {
        skipDataValidation: false,
        faucetURL: DEFAULT_FAUCET_URL,
      });
}

export async function setup(providerType: ProviderType = 'rpc') {
  const keypair = Ed25519Keypair.generate();
  const address = keypair.getPublicKey().toSuiAddress();
  const provider = getProvider(providerType);
  await retry(() => provider.requestSuiFromFaucet(address), {
    backoff: 'EXPONENTIAL',
    // overall timeout in 60 seconds
    timeout: 1000 * 60,
    // skip retry if we hit the rate-limit error
    retryIf: (error: any) => !(error instanceof FaucetRateLimitError),
    logger: (msg) => console.warn('Retrying requesting from faucet: ' + msg),
  });

  return new TestToolbox(keypair, provider);
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
  const publishTxn = await signer.publish({
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
