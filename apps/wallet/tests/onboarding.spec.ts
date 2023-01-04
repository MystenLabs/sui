// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import 'tsconfig-paths/register';
import * as bcs from '@mysten/bcs';
import { test, expect } from './fixtures';
import { Ed25519Keypair } from '@mysten/sui.js';
import { generateMnemonic } from '@scure/bip39';
import { wordlist } from '@scure/bip39/wordlists/english';

console.log(bcs, Ed25519Keypair);

// test('create new wallet', async ({ page, extensionUrl }) => {
//     await page.goto(extensionUrl);
//     await page.getByRole('link', { name: /Get Started/ }).click();
//     await page.getByRole('link', { name: /Create a New Wallet/ }).click();
//     await page.getByLabel('Create Password').fill('mystenlabs');
//     await page.getByLabel('Confirm Password').fill('mystenlabs');
//     // TODO: Clicking checkbox should be improved:
//     await page
//         .locator('label', { has: page.locator('input[type=checkbox]') })
//         .locator('span')
//         .nth(0)
//         .click();
//     await page.getByRole('button', { name: /Create Wallet/ }).click();
//     await page.getByRole('button', { name: /Open Sui Wallet/ }).click();
//     await expect(page.getByRole('main')).toBeVisible();
// });

// test.only('import wallet', async ({ page, extensionUrl }) => {
//     const mnemonic = generateMnemonic(wordlist);
//     const keypair = Ed25519Keypair.deriveKeypair(mnemonic);

//     await page.goto(extensionUrl);
//     await page.getByRole('link', { name: /Get Started/ }).click();
//     await page.getByRole('link', { name: /Import an Existing Wallet/ }).click();
//     await page.getByLabel('Enter Recovery Phrase').fill(mnemonic);
//     await page.pause();
//     // await page.getByRole('link', { name: /Create a New Wallet/ }).click();
//     // // TODO: Clicking checkbox should be improved:
//     // await page
//     //     .locator('label', { has: page.locator('input[type=checkbox]') })
//     //     .locator('span')
//     //     .nth(0)
//     //     .click();
//     // await page.getByRole('button', { name: /Create Wallet/ }).click();
//     // await page.getByRole('button', { name: /Open Sui Wallet/ }).click();
//     // await expect(page.getByRole('main')).toBeVisible();
// });
