// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Base64DataBuffer,
  DEFAULT_SECP256K1_DERIVATION_PATH,
  Secp256k1Keypair,
} from '../../../src';
import { describe, it, expect } from 'vitest';
import * as secp from '@noble/secp256k1';
import { Signature } from '@noble/secp256k1';
import { fromB64, toB64 } from '@mysten/bcs';

// Test case from https://github.com/rust-bitcoin/rust-secp256k1/blob/master/examples/sign_verify.rs#L26
const VALID_SECP256K1_SECRET_KEY = [
  59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203,
  174, 253, 102, 39, 170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
];

// Corresponding to the secret key above.
export const VALID_SECP256K1_PUBLIC_KEY = [
  2, 29, 21, 35, 7, 198, 183, 43, 14, 208, 65, 139, 14, 112, 205, 128, 231, 245,
  41, 91, 141, 134, 245, 114, 45, 63, 82, 19, 251, 210, 57, 79, 54,
];

// Invalid private key with incorrect length
export const INVALID_SECP256K1_SECRET_KEY = Uint8Array.from(Array(31).fill(1));

// Invalid public key with incorrect length
export const INVALID_SECP256K1_PUBLIC_KEY = Uint8Array.from(Array(32).fill(1));

const TEST_MNEMONIC =
  'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss';

describe('secp256k1-keypair', () => {
  it('new keypair', () => {
    const keypair = new Secp256k1Keypair();
    expect(keypair.getPublicKey().toBytes().length).toBe(33);
    expect(2).toEqual(2);
  });

  it('create keypair from secret key', () => {
    const secret_key = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
    const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const keypair = Secp256k1Keypair.fromSecretKey(secret_key);
    expect(keypair.getPublicKey().toBytes()).toEqual(new Uint8Array(pub_key));
    expect(keypair.getPublicKey().toBase64()).toEqual(pub_key_base64);
  });

  it('creating keypair from invalid secret key throws error', () => {
    const secret_key = new Uint8Array(INVALID_SECP256K1_SECRET_KEY);
    let secret_key_base64 = toB64(secret_key);
    const secretKey = fromB64(secret_key_base64);
    expect(() => {
      Secp256k1Keypair.fromSecretKey(secretKey);
    }).toThrow('Expected 32 bytes of private key');
  });

  it('generate keypair from random seed', () => {
    const keypair = Secp256k1Keypair.fromSeed(
      Uint8Array.from(Array(32).fill(8))
    );
    expect(keypair.getPublicKey().toBase64()).toEqual(
      'A/mR+UTR4ZVKf8i5v2Lg148BX0wHdi1QXiDmxFJgo2Yb'
    );
  });

  it('signature of data is valid', async () => {
    const keypair = new Secp256k1Keypair();
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('hello world')
    );

    const msgHash = await secp.utils.sha256(signData.getData());
    const sig = keypair.signData(signData, false);
    expect(secp.verify(Signature.fromCompact(sig.getData()), msgHash, keypair.getPublicKey().toBytes())).toBeTruthy();
  });

  it('signature of data is same as rust implementation', async () => {
    const secret_key = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
    const keypair = Secp256k1Keypair.fromSecretKey(secret_key);
    const signData = new Base64DataBuffer(
      new TextEncoder().encode('Hello, world!')
    );

    const msgHash = await secp.utils.sha256(signData.getData());
    const sig = keypair.signData(signData, false);
    
    // Assert the signature is the same as the rust implementation. See https://github.com/MystenLabs/fastcrypto/blob/0436d6ef11684c291b75c930035cb24abbaf581e/fastcrypto/src/tests/secp256k1_tests.rs#L115
    expect(Buffer.from(sig.getData()).toString('hex')).toEqual('25d450f191f6d844bf5760c5c7b94bc67acc88be76398129d7f43abdef32dc7f7f1a65b7d65991347650f3dd3fa3b3a7f9892a0608521cbcf811ded433b31f8b')
    expect(secp.verify(Signature.fromCompact(sig.getData()), msgHash, keypair.getPublicKey().toBytes())).toBeTruthy();
  });

  it('invalid mnemonics to derive secp256k1 keypair', () => {
    expect(() => {
      Secp256k1Keypair.deriveKeypair(DEFAULT_SECP256K1_DERIVATION_PATH, 'aaa');
    }).toThrow('Invalid mnemonic');
  });

  it('derive secp256k1 keypair from path and mnemonics', () => {
    // Test case generated against rust: /sui/crates/sui/src/unit_tests/keytool_tests.rs#L149
    const keypair = Secp256k1Keypair.deriveKeypair(
      DEFAULT_SECP256K1_DERIVATION_PATH,
      TEST_MNEMONIC
    );

    expect(keypair.getPublicKey().toBase64()).toEqual(
      'A+NxdDVYKrM9LjFdIem8ThlQCh/EyM3HOhU2WJF3SxMf'
    );
    expect(keypair.getPublicKey().toSuiAddress()).toEqual(
      'ed17b3f435c03ff69c2cdc6d394932e68375f20f'
    );
  });

  it('incorrect purpose node for secp256k1 derivation path', () => {
    expect(() => {
      Secp256k1Keypair.deriveKeypair(`m/44'/784'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });

  it('incorrect hardened path for secp256k1 key derivation', () => {
    expect(() => {
      Secp256k1Keypair.deriveKeypair(`m/54'/784'/0'/0'/0'`, TEST_MNEMONIC);
    }).toThrow('Invalid derivation path');
  });
});
