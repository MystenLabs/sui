// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import bip39 from 'bip39-light';

export function generateMnemonic(): string {
  return bip39.generateMnemonic();
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
 * Parse and validate a path that is compliant to BIP-32 in form m/54'/784'/{account_index}'/{change_index}/{address_index}
 * Note that the purpose for Secp256k1 is registered as 54, to differentiate from Ed25519 with purpose 44.
 *
 * @param path path string (e.g. `m/54'/784'/0'/0/0`)
 */
export function isValidBIP32Path(path: string): boolean {
  if (!new RegExp("^m\\/54'\\/784'\\/[0-9]+'\\/[0-9]+\\/[0-9]+$").test(path)) {
    return false;
  }
  return true;
}

export const validateMnemonics = bip39.validateMnemonic;
