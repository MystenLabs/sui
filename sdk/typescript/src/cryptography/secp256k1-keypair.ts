// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as secp from '@noble/secp256k1';
import type { ExportedKeypair, Keypair } from './keypair';
import { PublicKey } from './publickey';
import { hmac } from '@noble/hashes/hmac';
import { sha256 } from '@noble/hashes/sha256';
import { Secp256k1PublicKey } from './secp256k1-publickey';
import { Signature } from '@noble/secp256k1';
import { isValidBIP32Path, mnemonicToSeed } from './mnemonics';
import { HDKey } from '@scure/bip32';
import { toB64 } from '@mysten/bcs';
import { SignatureScheme } from './signature';
import { bytesToHex } from '@noble/hashes/utils';
import { blake2b } from '@noble/hashes/blake2b';

export const DEFAULT_SECP256K1_DERIVATION_PATH = "m/54'/784'/0'/0/0";

secp.utils.hmacSha256Sync = (key: Uint8Array, ...msgs: Uint8Array[]) => {
  const h = hmac.create(sha256, key);
  msgs.forEach((msg) => h.update(msg));
  return h.digest();
};

/**
 * Secp256k1 Keypair data
 */
export interface Secp256k1KeypairData {
  publicKey: Uint8Array;
  secretKey: Uint8Array;
}

/**
 * An Secp256k1 Keypair used for signing transactions.
 */
export class Secp256k1Keypair implements Keypair {
  private keypair: Secp256k1KeypairData;

  /**
   * Create a new keypair instance.
   * Generate random keypair if no {@link Secp256k1Keypair} is provided.
   *
   * @param keypair secp256k1 keypair
   */
  constructor(keypair?: Secp256k1KeypairData) {
    if (keypair) {
      this.keypair = keypair;
    } else {
      const secretKey: Uint8Array = secp.utils.randomPrivateKey();
      const publicKey: Uint8Array = secp.getPublicKey(secretKey, true);

      this.keypair = { publicKey, secretKey };
    }
  }

  /**
   * Get the key scheme of the keypair Secp256k1
   */
  getKeyScheme(): SignatureScheme {
    return 'Secp256k1';
  }

  /**
   * Generate a new random keypair
   */
  static generate(): Secp256k1Keypair {
    const secretKey = secp.utils.randomPrivateKey();
    const publicKey = secp.getPublicKey(secretKey, true);

    return new Secp256k1Keypair({ publicKey, secretKey });
  }

  /**
   * Create a keypair from a raw secret key byte array.
   *
   * This method should only be used to recreate a keypair from a previously
   * generated secret key. Generating keypairs from a random seed should be done
   * with the {@link Keypair.fromSeed} method.
   *
   * @throws error if the provided secret key is invalid and validation is not skipped.
   *
   * @param secretKey secret key byte array
   * @param options: skip secret key validation
   */

  static fromSecretKey(
    secretKey: Uint8Array,
    options?: { skipValidation?: boolean },
  ): Secp256k1Keypair {
    const publicKey: Uint8Array = secp.getPublicKey(secretKey, true);
    if (!options || !options.skipValidation) {
      const encoder = new TextEncoder();
      const signData = encoder.encode('sui validation');
      const msgHash = bytesToHex(blake2b(signData, { dkLen: 32 }));
      const signature = secp.signSync(msgHash, secretKey);
      if (!secp.verify(signature, msgHash, publicKey, { strict: true })) {
        throw new Error('Provided secretKey is invalid');
      }
    }
    return new Secp256k1Keypair({ publicKey, secretKey });
  }

  /**
   * Generate a keypair from a 32 byte seed.
   *
   * @param seed seed byte array
   */
  static fromSeed(seed: Uint8Array): Secp256k1Keypair {
    let publicKey = secp.getPublicKey(seed, true);
    return new Secp256k1Keypair({ publicKey, secretKey: seed });
  }

  /**
   * The public key for this keypair
   */
  getPublicKey(): PublicKey {
    return new Secp256k1PublicKey(this.keypair.publicKey);
  }

  /**
   * Return the signature for the provided data.
   */
  signData(data: Uint8Array, useRecoverable: boolean): Uint8Array {
    const msgHash = sha256(data);
    // Starting from sui 0.25.0, sui accepts 64-byte nonrecoverable signature instead of 65-byte recoverable signature for Secp256k1.
    // TODO(joyqvq): Remove recoverable signature support after 0.25.0 is released.
    if (useRecoverable) {
      const [sig, rec_id] = secp.signSync(msgHash, this.keypair.secretKey, {
        canonical: true,
        recovered: true,
      });
      var recoverable_sig = new Uint8Array(65);
      recoverable_sig.set(Signature.fromDER(sig).toCompactRawBytes());
      recoverable_sig.set([rec_id], 64);
      return recoverable_sig;
    } else {
      const sig = secp.signSync(msgHash, this.keypair.secretKey, {
        canonical: true,
        recovered: false,
      });
      return Signature.fromDER(sig).toCompactRawBytes();
    }
  }

  /**
   * Derive Secp256k1 keypair from mnemonics and path. The mnemonics must be normalized
   * and validated against the english wordlist.
   *
   * If path is none, it will default to m/54'/784'/0'/0/0, otherwise the path must
   * be compliant to BIP-32 in form m/54'/784'/{account_index}'/{change_index}/{address_index}.
   */
  static deriveKeypair(mnemonics: string, path?: string): Secp256k1Keypair {
    if (path == null) {
      path = DEFAULT_SECP256K1_DERIVATION_PATH;
    }
    if (!isValidBIP32Path(path)) {
      throw new Error('Invalid derivation path');
    }
    const key = HDKey.fromMasterSeed(mnemonicToSeed(mnemonics)).derive(path);
    if (key.publicKey == null || key.privateKey == null) {
      throw new Error('Invalid key');
    }
    return new Secp256k1Keypair({
      publicKey: key.publicKey,
      secretKey: key.privateKey,
    });
  }

  export(): ExportedKeypair {
    return {
      schema: 'Secp256k1',
      privateKey: toB64(this.keypair.secretKey),
    };
  }
}
