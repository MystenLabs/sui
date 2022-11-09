// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import passworder from '@metamask/browser-passworder';

type Serializable =
    | string
    | number
    | boolean
    | { [index: string]: Serializable };

export async function encrypt(
    password: string,
    secrets: Serializable
): Promise<string> {
    return passworder.encrypt(password, secrets);
}

export async function decrypt<T extends Serializable>(
    password: string,
    ciphertext: string
): Promise<T> {
    return await passworder.decrypt(password, ciphertext);
}
