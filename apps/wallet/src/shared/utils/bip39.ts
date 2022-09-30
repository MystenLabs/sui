// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    generateMnemonic as bip39GenerateMnemonic,
    validateMnemonic as bip39ValidateMnemonic,
} from '@scure/bip39';
import { wordlist } from '@scure/bip39/wordlists/english';

/**
 * Generate mnemonics as 12 words string using the english wordlist.
 *
 * @returns a 12 words string separated by spaces.
 */
export function generateMnemonic(): string {
    return bip39GenerateMnemonic(wordlist);
}

/**
 * Validate a mnemonic string in the BIP39 English wordlist.
 *
 * @param mnemonics a words string split by spaces of length 12/15/18/21/24.
 *
 * @returns true if the mnemonic is valid, false otherwise.
 */
export function validateMnemonics(mnemonics: string): boolean {
    return bip39ValidateMnemonic(mnemonics, wordlist);
}

/**
 * Sanitize the mnemonics string provided by user.
 *
 * @param mnemonics a 12-word string split by spaces that may contain mixed cases
 * and extra spaces.
 *
 * @returns a sanitized mnemonics string.
 */
export function normalizeMnemonics(mnemonics: string): string {
    return mnemonics
        .trim()
        .split(/\s+/)
        .map((part) => part.toLowerCase())
        .join(' ');
}
