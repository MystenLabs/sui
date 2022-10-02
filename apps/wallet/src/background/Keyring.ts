// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js';
import { BehaviorSubject } from 'rxjs';
import Browser from 'webextension-polyfill';

import { encrypt, decrypt } from '_shared/cryptography/keystore';
import { generateMnemonic } from '_shared/utils/bip39';

import type { Keypair } from '@mysten/sui.js';

const STORAGE_KEY = 'vault';

class Keyring {
    private readonly _locked = new BehaviorSubject<boolean>(true);
    private _encryptedMnemonic: Promise<string | null>;
    private _keypair: Keypair | null = null;

    constructor() {
        this._encryptedMnemonic = this.loadMnemonic();
    }

    // Creates a new mnemonic and saves it to storage encrypted
    public async createMnemonic(password: string) {
        if (await this.isWalletInitialized()) {
            throw new Error(
                'Mnemonic already exists, creating a new one will override it. Clear the existing one first.'
            );
        }
        const encryptedMnemonic = await encrypt(
            password,
            Buffer.from(generateMnemonic(), 'utf8')
        );
        await this.storeEncryptedMnemonic(encryptedMnemonic);
        this._encryptedMnemonic = Promise.resolve(encryptedMnemonic);
    }

    public lock() {
        this._keypair = null;
        this._locked.next(true);
    }

    public async unlock(password: string) {
        this._keypair = Ed25519Keypair.deriveKeypair(
            await this.decryptMnemonic(password)
        );
        this._locked.next(false);
    }

    public exportMnemonic(password: string) {
        return this.decryptMnemonic(password);
    }

    public async clearMnemonic() {
        this._encryptedMnemonic = Promise.resolve(null);
        await this.storeEncryptedMnemonic(null);
        this.lock();
    }

    public async isWalletInitialized() {
        return !!(await this._encryptedMnemonic);
    }

    public get isLocked() {
        return this._locked.asObservable();
    }

    public get keypair() {
        return this._keypair;
    }

    // sui address always prefixed with 0x
    public get address() {
        if (this._keypair) {
            let address = this._keypair.getPublicKey().toSuiAddress();
            if (!address.startsWith('0x')) {
                address = `0x${address}`;
            }
            return address;
        }
        return null;
    }

    // pass null to delete it
    private async storeEncryptedMnemonic(encryptedMnemonic: string | null) {
        await Browser.storage.local.set({ [STORAGE_KEY]: encryptedMnemonic });
    }

    private async loadMnemonic() {
        const storedMnemonic = await Browser.storage.local.get({
            [STORAGE_KEY]: null,
        });
        return storedMnemonic[STORAGE_KEY];
    }

    private async decryptMnemonic(password: string) {
        const encryptedMnemonic = await this._encryptedMnemonic;
        if (!encryptedMnemonic) {
            throw new Error(
                'Mnemonic is not initialized. Create a new one first.'
            );
        }
        return Buffer.from(await decrypt(password, encryptedMnemonic)).toString(
            'utf8'
        );
    }
}

export default new Keyring();
