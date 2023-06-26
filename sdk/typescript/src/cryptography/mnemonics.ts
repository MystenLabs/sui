// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { toHEX } from '@mysten/bcs';
import { mnemonicToSeedSync as bip39MnemonicToSeedSync } from '@scure/bip39';

/**
 * Parse and validate a path that is compliant to SLIP-0010 in form m/44'/784'/{account_index}'/{change_index}'/{address_index}'.
 *
 * @param path path string (e.g. `m/44'/784'/0'/0'/0'`).
 */
export function isValidHardenedPath(path: string): boolean {
	if (!new RegExp("^m\\/44'\\/784'\\/[0-9]+'\\/[0-9]+'\\/[0-9]+'+$").test(path)) {
		return false;
	}
	return true;
}

/**
 * Parse and validate a path that is compliant to BIP-32 in form m/54'/784'/{account_index}'/{change_index}/{address_index}
 * for Secp256k1 and m/74'/784'/{account_index}'/{change_index}/{address_index} for Secp256r1.
 *
 * Note that the purpose for Secp256k1 is registered as 54, to differentiate from Ed25519 with purpose 44.
 *
 * @param path path string (e.g. `m/54'/784'/0'/0/0`).
 */
export function isValidBIP32Path(path: string): boolean {
	if (!new RegExp("^m\\/(54|74)'\\/784'\\/[0-9]+'\\/[0-9]+\\/[0-9]+$").test(path)) {
		return false;
	}
	return true;
}

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
