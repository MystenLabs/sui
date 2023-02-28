// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect } from 'vitest';
import {
  Ed25519Keypair,
  getEvents,
  getExecutionStatusType,
  JsonRpcProvider,
  ObjectId,
  RawSigner,
  fromB64,
  localnetConnection,
  Connection,
} from '../../../src';
import { retry } from 'ts-retry-promise';
import { FaucetRateLimitError } from '../../../src/rpc/faucet-client';

const TEST_ENDPOINTS = localnetConnection;
const DEFAULT_FAUCET_URL =
  import.meta.env.VITE_FAUCET_URL ?? TEST_ENDPOINTS.faucet;
const DEFAULT_FULLNODE_URL =
  import.meta.env.VITE_FULLNODE_URL ?? TEST_ENDPOINTS.fullnode;
const SUI_BIN = import.meta.env.VITE_SUI_BIN ?? 'cargo run --bin sui';

export const DEFAULT_RECIPIENT = '0x36096be6a0314052931babed39f53c0666a6b0df';
export const DEFAULT_RECIPIENT_2 = '0x46096be6a0314052931babed39f53c0666a6b0da';
export const DEFAULT_GAS_BUDGET = 10000;

export class TestToolbox {
  constructor(
    public keypair: Ed25519Keypair,
    public provider: JsonRpcProvider,
  ) {}

  address() {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  public async getActiveValidators() {
    return this.provider.getValidators();
  }
}

export function getProvider(): JsonRpcProvider {
  return new JsonRpcProvider(
    new Connection({
      fullnode: DEFAULT_FULLNODE_URL,
      faucet: DEFAULT_FAUCET_URL,
    }),
    {
      skipDataValidation: false,
    },
  );
}

export async function setup() {
  const keypair = Ed25519Keypair.generate();
  const address = keypair.getPublicKey().toSuiAddress();
  const provider = getProvider();
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
  packagePath: string,
): Promise<ObjectId> {
  const { execSync } = require('child_process');
  const compiledModules = JSON.parse(
    execSync(
      `${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath}`,
      { encoding: 'utf-8' },
    ),
  );
  const publishTxn = await signer.publish({
    compiledModules: useLocalTxnBuilder
      ? compiledModules.map((m: any) => Array.from(fromB64(m)))
      : compiledModules,
    gasBudget: DEFAULT_GAS_BUDGET,
  });
  expect(getExecutionStatusType(publishTxn)).toEqual('success');

  const publishEvent = getEvents(publishTxn)?.find((e) => 'publish' in e);

  // @ts-ignore: Publish not narrowed:
  const packageId = publishEvent?.publish.packageId.replace(/^(0x)(0+)/, '0x');
  console.info(
    `Published package ${packageId} from address ${await signer.getAddress()}}`,
  );
  return packageId;
}
