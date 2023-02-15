// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import nacl from 'tweetnacl';
import { describe, it, expect, beforeAll } from 'vitest';
import {
  Ed25519Keypair,
  IntentScope,
  messageWithIntent,
  RawSigner,
  Secp256k1Keypair,
  versionToString,
} from '../../src';
import * as secp from '@noble/secp256k1';
import { Signature } from '@noble/secp256k1';
import { setup, TestToolbox } from './utils/setup';
import { gt } from '@suchipi/femver';
import { fromB64, toB64 } from '@mysten/bcs';

describe('RawSigner', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Ed25519 keypair signData', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const { signature, pubKey } = await signer.signData(signData);
    const isValid = nacl.sign.detached.verify(
      signData,
      fromB64(signature),
      fromB64(pubKey),
    );
    expect(isValid).toBeTruthy();
  });

  it('Ed25519 keypair signMessage', async () => {
    const keypair = new Ed25519Keypair();
    const signData = new TextEncoder().encode('hello world');
    const signer = new RawSigner(keypair, toolbox.provider);
    const { messageBytes, signature } = await signer.signMessage(signData);
    const isValid = nacl.sign.detached.verify(
      fromB64(messageBytes),
      fromB64(signature.signature),
      fromB64(signature.pubKey),
    );
    expect(isValid).toBeTruthy();
  });

  it('Secp256k1 keypair signData', async () => {
    const keypair = new Secp256k1Keypair();
    const signData = new TextEncoder().encode('hello world');
    const msgHash = await secp.utils.sha256(signData);
    const signer = new RawSigner(keypair, toolbox.provider);
    const { signature, pubKey } = await signer.signData(signData);

    const version = await toolbox.provider.getRpcApiVersion();
    // TODO(joyqvq): Remove recoverable signature test once 0.25.0 is released.
    let useRecoverable =
      version && gt(versionToString(version), '0.24.0') ? false : true;
    if (useRecoverable) {
      const recovered_pubkey = secp.recoverPublicKey(
        msgHash,
        Signature.fromCompact(fromB64(signature).slice(0, 64)),
        fromB64(signature)[64],
        true,
      );
      const expected = keypair.getPublicKey().toBase64();
      expect(pubKey).toEqual(expected);
      expect(toB64(recovered_pubkey)).toEqual(expected);
    } else {
      expect(
        secp.verify(
          Signature.fromCompact(fromB64(signature)),
          msgHash,
          fromB64(pubKey),
        ),
      ).toBeTruthy();
    }
  });
});
