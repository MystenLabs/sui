// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import passworder from '@metamask/browser-passworder';
export async function encrypt(
    password: string,
    secrets: Buffer
): Promise<string> {
    return passworder.encrypt(password, secrets);
}

export async function decrypt(
    password: string,
    ciphertext: string
): Promise<Buffer> {
    return passworder.decrypt(password, ciphertext);
}
