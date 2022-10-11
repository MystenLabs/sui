// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js';
import mitt from 'mitt';
import Browser from 'webextension-polyfill';

import { encrypt, decrypt } from '_shared/cryptography/keystore';
import { generateMnemonic } from '_shared/utils/bip39';

import type { Keypair } from '@mysten/sui.js';

type KeyringEvents = {
    lockedStatusUpdate: boolean;
};

const STORAGE_KEY = 'vault';

class Keyring {
    #events = mitt<KeyringEvents>();
    #locked = true;
    #keypair: Keypair | null = null;
    #mnemonic: string | null = null;

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
    }

    public lock() {
        this.#keypair = null;
        this.#mnemonic = null;
        this.#locked = true;
        this.notifyLockedStatusUpdate(this.#locked);
    }

    public async unlock(password: string) {
        this.#mnemonic = await this.decryptMnemonic(password);
        this.#keypair = Ed25519Keypair.deriveKeypair(this.#mnemonic);
        this.#locked = false;
        this.notifyLockedStatusUpdate(this.#locked);
    }

    public async clearMnemonic() {
        await this.storeEncryptedMnemonic(null);
        this.lock();
    }

    public async isWalletInitialized() {
        return !!(await this.loadMnemonic());
    }

    public get isLocked() {
        return this.#locked;
    }

    public get keypair() {
        return this.#keypair;
    }

    public mnemonic() {
        return this.#mnemonic;
    }

    // sui address always prefixed with 0x
    public get address() {
        if (this.#keypair) {
            let address = this.#keypair.getPublicKey().toSuiAddress();
            if (!address.startsWith('0x')) {
                address = `0x${address}`;
            }
            return address;
        }
        return null;
    }

    public on = this.#events.on;

    public off = this.#events.off;

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
        const encryptedMnemonic = await this.loadMnemonic();
        if (!encryptedMnemonic) {
            throw new Error(
                'Mnemonic is not initialized. Create a new one first.'
            );
        }
        return Buffer.from(await decrypt(password, encryptedMnemonic)).toString(
            'utf8'
        );
    }

    private notifyLockedStatusUpdate(isLocked: boolean) {
        this.#events.emit('lockedStatusUpdate', isLocked);
    }
}

export default new Keyring();
