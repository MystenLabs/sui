// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import nacl from 'tweetnacl';
import { describe, it, expect } from 'vitest';
import { Base64DataBuffer, Ed25519Keypair } from '../../../src';

const VALID_SECRET_KEY =
  'mdqVWeFekT7pqy5T49+tV12jO0m+ESW7ki4zSU9JiCgbL0kJbj5dvQ/PqcDAzZLZqzshVEs01d1KZdmLh4uZIg==';
const INVALID_SECRET_KEY =
  'mdqVWeFekT7pqy5T49+tV12jO0m+ESW7ki4zSU9JiCgbL0kJbj5dvQ/PqcDAzZLZqzshVEs01d1KZdmLh4uZIG==';
const TEST_MNEMONIC =
  'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss';

describe('ed25519-keypair', () => {
  it('new keypair', () => {
    const keypair = new Ed25519Keypair();
    expect(keypair.getPublicKey().toBytes().length).toBe(32);
    expect(2).toEqual(2);
  });

  it('create keypair from secret key', () => {
    const secretKey = Buffer.from(VALID_SECRET_KEY, 'base64');
    const keypair = Ed25519Keypair.fromSecretKey(secretKey);
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'Gy9JCW4+Xb0Pz6nAwM2S2as7IVRLNNXdSmXZi4eLmSI='
    );
  });

  it('creating keypair from invalid secret key throws error', () => {
    const secretKey = Buffer.from(INVALID_SECRET_KEY, 'base64');
    expect(() => {
      Ed25519Keypair.fromSecretKey(secretKey);
    }).toThrow('provided secretKey is invalid');
  });

  it('creating keypair from invalid secret key succeeds if validation is skipped', () => {
    const secretKey = Buffer.from(INVALID_SECRET_KEY, 'base64');
    const keypair = Ed25519Keypair.fromSecretKey(secretKey, {
      skipValidation: true,
    });
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'Gy9JCW4+Xb0Pz6nAwM2S2as7IVRLNNXdSmXZi4eLmSA='
    );
  });

  it('generate keypair from random seed', () => {
    const keypair = Ed25519Keypair.fromSeed(Uint8Array.from(Array(32).fill(8)));
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'E5j2LG0aRXxRumpLXz29L2n8qTIWIY3ImX5Ba9F9k8o='
    );
  });

  it('signature of data is valid', () => {
    const keypair = new Ed25519Keypair();
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('hello world')
    );
    const signature = keypair.signData(signData);
    const isValid = nacl.sign.detached.verify(
      signData.getData(),
      signature.getData(),
      keypair.getPublicKey().toBytes()
    );
    expect(isValid).toBeTruthy();
  });

  it('derive ed25519 keypair from path and mnemonics', () => {
    // Test case generated against rust: /sui/crates/sui/src/unit_tests/keytool_tests.rs#L149
    const keypair = Ed25519Keypair.deriveKeypair(TEST_MNEMONIC);
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'aFstb5h4TddjJJryHJL1iMob6AxAqYxVv3yRt05aweI='
    );
    expect(keypair.getPublicKey().toSuiAddress()).toEqual(
      '1a4623343cd42be47d67314fce0ad042f3c82685'
    );
  });

  it('incorrect coin type node for ed25519 derivation path', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair(`m/44'/0'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });

  it('incorrect purpose node for ed25519 derivation path', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair(`m/54'/784'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });

  it('invalid mnemonics to derive ed25519 keypair', () => {
    expect(() => {
      Ed25519Keypair.deriveKeypair('aaa');
    }).toThrow('Invalid mnemonic');
  });
});
