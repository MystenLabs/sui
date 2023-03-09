// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import nacl from 'tweetnacl';
import { describe, it, expect, beforeAll } from 'vitest';
import {
  Ed25519Keypair,
  fromSerializedSignature,
  RawSigner,
  Secp256k1Keypair,
  verifyMessage,
} from '../../src';
import * as secp from '@noble/secp256k1';
import { Signature } from '@noble/secp256k1';
import { setup, TestToolbox } from './utils/setup';

describe('RawSigner', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Ed25519 keypair signData', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const serializedSignature = await signer.signData(signData);
    const { signature, pubKey } = fromSerializedSignature(serializedSignature);
    const isValid = nacl.sign.detached.verify(
      signData,
      signature,
      pubKey.toBytes(),
    );
    expect(isValid).toBeTruthy();
  });

  it('Ed25519 keypair signMessage', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const { signature } = await signer.signMessage(signData);
    const isValid = await verifyMessage(signData, signature);
    expect(isValid).toBe(true);
  });

  it('Ed25519 keypair invalid signMessage', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const { signature } = await signer.signMessage(signData);
    const isValid = await verifyMessage(
      new TextEncoder().encode('hello worlds'),
      signature,
    );
    expect(isValid).toBe(false);
  });

  it('Secp256k1 keypair signData', async () => {
    const keypair = new Secp256k1Keypair();
    const signData = new TextEncoder().encode('hello world');
    const msgHash = await secp.utils.sha256(signData);
    const signer = new RawSigner(keypair, toolbox.provider);
    const serializedSignature = await signer.signData(signData);
    const { signature, pubKey } = fromSerializedSignature(serializedSignature);

    expect(
      secp.verify(Signature.fromCompact(signature), msgHash, pubKey.toBytes()),
    ).toBeTruthy();
  });

  it('Secp256k1 keypair signMessage', async () => {
    const keypair = new Secp256k1Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const { signature } = await signer.signMessage(signData);

    const isValid = await verifyMessage(signData, signature);
    expect(isValid).toBe(true);
  });
});
