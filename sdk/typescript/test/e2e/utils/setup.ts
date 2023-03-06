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

export const DEFAULT_RECIPIENT =
  '0x0c567ffdf8162cb6d51af74be0199443b92e823d4ba6ced24de5c6c463797d46';
export const DEFAULT_RECIPIENT_2 =
  '0xbb967ddbebfee8c40d8fdd2c24cb02452834cd3a7061d18564448f900eb9e66d';
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
  packagePath: string,
): Promise<ObjectId> {
  const { execSync } = require('child_process');
  const tmp = require('tmp');
  // remove all controlled temporary objects on process exit
  tmp.setGracefulCleanup();

  const tmpobj = tmp.dirSync({ unsafeCleanup: true });

  const compiledModules = JSON.parse(
    execSync(
      `${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath} --install-dir ${tmpobj.name}`,
      { encoding: 'utf-8' },
    ),
  );
  const publishTxn = await signer.signAndExecuteTransaction({
    kind: 'publish',
    data: {
      compiledModules: compiledModules.map((m: any) => Array.from(fromB64(m))),
      gasBudget: DEFAULT_GAS_BUDGET,
    },
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
