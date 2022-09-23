// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare module 'bip39-light' {
    export function entropyToMnemonic(
        entropy: Buffer | string,
        wordlist?: string[]
    ): string;
    export function generateMnemonic(
        strength?: number,
        rng?: (size: number) => Buffer,
        wordlist?: string[]
    ): string;
    export function mnemonicToSeed(mnemonic: string, password?: string): Buffer;
    export function validateMnemonic(mnemonic: string): boolean;
    export function mnemonicToSeedHex(
        mnemonic: string,
        password?: string
    ): string;
}
