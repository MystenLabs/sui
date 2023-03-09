// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect } from 'vitest';
import { execSync } from 'child_process';
import tmp from 'tmp';

import {
  Ed25519Keypair,
  getEvents,
  getExecutionStatusType,
  JsonRpcProvider,
  fromB64,
  localnetConnection,
  Connection,
  Coin,
  Transaction,
  Commands,
  RawSigner,
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
  keypair: Ed25519Keypair;
  provider: JsonRpcProvider;
  signer: RawSigner;

  constructor(keypair: Ed25519Keypair, provider: JsonRpcProvider) {
    this.keypair = keypair;
    this.provider = provider;
    this.signer = new RawSigner(this.keypair, this.provider);
  }

  address() {
    return this.keypair.getPublicKey().toSuiAddress();
  }

  async getGasObjectsOwnedByAddress() {
    const objects = await this.provider.getObjectsOwnedByAddress(
      this.address(),
    );

    return objects.filter((obj) => Coin.isSUI(obj));
  }

  public async getActiveValidators() {
    return (await this.provider.getLatestSuiSystemState()).activeValidators;
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
  packagePath: string,
  toolbox?: TestToolbox,
) {
  // TODO: We create a unique publish address per publish, but we really could share one for all publishes.
  if (!toolbox) {
    toolbox = await setup();
  }

  // remove all controlled temporary objects on process exit
  tmp.setGracefulCleanup();

  const tmpobj = tmp.dirSync({ unsafeCleanup: true });

  const compiledModules = JSON.parse(
    execSync(
      `${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath} --install-dir ${tmpobj.name}`,
      { encoding: 'utf-8' },
    ),
  );
  const tx = new Transaction();
  tx.setGasBudget(DEFAULT_GAS_BUDGET);
  const cap = tx.add(
    Commands.Publish(compiledModules.map((m: any) => Array.from(fromB64(m)))),
  );
  tx.add(
    Commands.MoveCall({
      target: '0x2::package::make_immutable',
      typeArguments: [],
      arguments: [cap],
    }),
  );

  const publishTxn = await toolbox.signer.signAndExecuteTransaction(tx);
  expect(getExecutionStatusType(publishTxn)).toEqual('success');

  const publishEvent = getEvents(publishTxn)?.find((e) => 'publish' in e);

  // @ts-ignore: Publish not narrowed:
  const packageId = publishEvent?.publish.packageId.replace(/^(0x)(0+)/, '0x');
  console.info(
    `Published package ${packageId} from address ${await toolbox.signer.getAddress()}}`,
  );

  return packageId;
}
