// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { toHEX } from '@mysten/bcs';
import { mnemonicToSeedSync as bip39MnemonicToSeedSync } from '@scure/bip39';

/**
 * Uses KDF to derive 64 bytes of key data from mnemonic with empty password.
 *
 * @param mnemonics 12 words string split by spaces.
 */
export function mnemonicToSeed(mnemonics: string): Uint8Array {
  return bip39MnemonicToSeedSync(mnemonics, '');
}

/**
 * Derive the seed in hex format from a 12-word mnemonic string.
 *
 * @param mnemonics 12 words string split by spaces.
 */
export function mnemonicToSeedHex(mnemonics: string): string {
  return toHEX(mnemonicToSeed(mnemonics));
}
