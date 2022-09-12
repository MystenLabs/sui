// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { HDKey } from '@scure/bip32';
import bip39 from 'bip39-light';
import { derivePath, getPublicKey } from 'ed25519-hd-key';
import nacl from 'tweetnacl';

import type { Ed25519KeypairData, Secp256k1KeypairData } from '@mysten/sui.js';

/**
 * Generate a 12-word random mnemonic and keypair using crypto.randomBytes
 * under the hood, defaults to 128-bits of entropy.
 * @returns a tuple of mnemonic and keypair. The mnemonics is a 12-word string
 * split by spaces.
 */
export function generateMnemonicsAndKeypair(): [string, Ed25519KeypairData] {
    const mnemonics = bip39.generateMnemonic();
    return [mnemonics, getKeypairFromMnemonics(mnemonics)];
}

export function generateMnemonic(): string {
    return bip39.generateMnemonic();
}

/**
 * Get public key and private key from the Mnemonics
 * @param mnemonics a 12-word seed phrase
 * @returns public key and private key
 */
export function getKeypairFromMnemonics(mnemonics: string): Ed25519KeypairData {
    const normalized = normalizeMnemonics(mnemonics);
    if (!validateMnemonics(normalized)) {
        throw new Error('Invalid mnemonics');
    }
    const seed = bip39.mnemonicToSeed(normalized);
    return nacl.sign.keyPair.fromSeed(
        // keyPair.fromSeed only takes a 32-byte array where `seed` is a 64-byte array
        new Uint8Array(seed.toJSON().data.slice(0, 32))
    );
}

/**
 * Sanitize the mnemonics string provided by user
 * @param mnemonics a 12-word string split by spaces that may contain mixed cases
 * and extra spaces
 * @returns a sanitized mnemonics string
 */
export function normalizeMnemonics(mnemonics: string): string {
    return mnemonics
        .trim()
        .split(/\s+/)
        .map((part) => part.toLowerCase())
        .join(' ');
}

/**
 * Parse and validate a path that is compliant to SLIP-0010 in form m/44'/784'/{account_index}'/{change_index}'/{address_index}'
 *
 * @param path path string (e.g. `m/44'/784'/0'/0'/0'`)
 */
export function isValidHardenedPath(path: string): boolean {
    if (!new RegExp("^m\\/44'\\/784'\\/0'\\/[0-9]+'\\/[0-9]+'+$").test(path)) {
        return false;
    }
    return true;
}

/**
 * Derive Ed25519 public key and private key from the Mnemonics using SLIP-0010 harden derivation path.
 * @param mnemonics a 12-word seed phrase
 * @param path path string (`m/44'/784'/0'/0'/0'`)
 * @returns public key and private key
 */
export function deriveKeypairFromMnemonics(
    path: string,
    mnemonics: string
): Ed25519KeypairData {
    if (!isValidHardenedPath(path)) {
        throw new Error('Invalid derivation path');
    }

    const normalized = normalizeMnemonics(mnemonics);
    if (!validateMnemonics(normalized)) {
        throw new Error('Invalid mnemonics');
    }

    const { key } = derivePath(path, bip39.mnemonicToSeedHex(normalized));

    return { publicKey: getPublicKey(key), secretKey: key };
}

/**
 * Parse and validate a path that is compliant to BIP-32 in form m/54'/784'/{account_index}'/{change_index}/{address_index}
 * Note that the purpose for Secp256k1 is registered as 54, to differentiate from Ed25519 with purpose 44.
 *
 * @param path path string (e.g. `m/54'/784'/0'/0/0`)
 */
export function isValidBIP32Path(path: string): boolean {
    if (
        !new RegExp("^m\\/54'\\/784'\\/[0-9]+'\\/[0-9]+\\/[0-9]+$").test(path)
    ) {
        return false;
    }
    return true;
}

/**
 * Derive Secp256k1 public key and private key from the Mnemonics using BIP32 derivation path.
 * @param mnemonics a 12-word seed phrase
 * @param path path string (`m/54'/784'/1'/0/0`)
 * @returns public key and private key
 */
export function deriveSecp256k1KeypairFromMnemonics(
    path: string,
    mnemonics: string
): Secp256k1KeypairData {
    if (!isValidBIP32Path(path)) {
        throw new Error('Invalid derivation path');
    }

    const normalized = normalizeMnemonics(mnemonics);
    if (!validateMnemonics(normalized)) {
        throw new Error('Invalid mnemonics');
    }
    const key = HDKey.fromMasterSeed(bip39.mnemonicToSeed(normalized)).derive(
        path
    );

    if (key.privateKey === null || key.publicKey === null) {
        throw new Error('Invalid derivation path');
    }

    return { publicKey: key.publicKey, secretKey: key.privateKey };
}

export const validateMnemonics = bip39.validateMnemonic;
