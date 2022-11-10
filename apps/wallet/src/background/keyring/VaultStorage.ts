// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { randomBytes } from '@noble/hashes/utils';
import Browser from 'webextension-polyfill';

import { Vault } from './Vault';
import { getRandomEntropy, toEntropy } from '_shared/utils/bip39';

import type { StoredData } from './Vault';
import type { Storage } from 'webextension-polyfill';

export const IS_SESSION_STORAGE_SUPPORTED = 'chrome' in global;
const SESSION_STORAGE: Storage.LocalStorageArea | null =
    // @ts-expect-error chrome
    IS_SESSION_STORAGE_SUPPORTED ? global.chrome.storage.session : null;
const LOCAL_STORAGE = Browser.storage.local;

// we use this password + a random one for each time we store the encrypted
// vault to session storage
const PASSWORD =
    process.env.WALLET_KEYRING_PASSWORD ||
    '344c6f7d04a65c24f35f5c710b0e91e2f2e2f88c038562622d5602019b937bc2c2aa2821e65cc94775fe5acf2fee240d38f1abbbe00b0e6682646a4ce10e908e';
const VAULT_KEY = 'vault';
const EPHEMERAL_PASSWORD_KEY = '244e4b24e667ebf';
const EPHEMERAL_VAULT_KEY = 'a8e451b8ae8a1b4';

async function getFromStorage<T>(
    storage: Storage.LocalStorageArea,
    key: string
): Promise<T | null> {
    return (await storage.get({ [key]: null }))[key];
}

async function setToStorage<T>(
    storage: Storage.LocalStorageArea,
    key: string,
    value: T
): Promise<void> {
    return await storage.set({ [key]: value });
}

async function ifSessionStorage(
    execFN: (sessionStorage: Storage.LocalStorageArea) => Promise<void>
) {
    if (SESSION_STORAGE) {
        return execFN(SESSION_STORAGE);
    }
}

function getRandomPassword() {
    return Buffer.from(randomBytes(64)).toString('hex');
}

function makeEphemeraPassword(rndPass: string) {
    return `${PASSWORD}${rndPass}`;
}

export class VaultStorage {
    #vault: Vault | null = null;

    /**
     * See {@link Keyring.createVault}
     * @param password
     * @param importedEntropy
     */
    public async create(password: string, importedEntropy?: string) {
        if (await this.isWalletInitialized()) {
            throw new Error(
                'Mnemonic already exists, creating a new one will override it. Clear the existing one first.'
            );
        }
        let vault: Vault | null = new Vault(
            importedEntropy ? toEntropy(importedEntropy) : getRandomEntropy()
        );
        await setToStorage(
            LOCAL_STORAGE,
            VAULT_KEY,
            await vault.encrypt(password)
        );
        vault = null;
    }

    public async unlock(password: string) {
        const encryptedVault = await getFromStorage<StoredData>(
            LOCAL_STORAGE,
            VAULT_KEY
        );
        if (!encryptedVault) {
            throw new Error(
                'Wallet is not initialized. Create a new one first.'
            );
        }
        this.#vault = await Vault.from(
            password,
            encryptedVault,
            async (aVault) =>
                setToStorage(
                    LOCAL_STORAGE,
                    VAULT_KEY,
                    await aVault.encrypt(password)
                )
        );
        await ifSessionStorage(async (sessionStorage) => {
            if (!this.#vault) {
                return;
            }
            const rndPass = getRandomPassword();
            const ephemeralPass = makeEphemeraPassword(rndPass);
            await setToStorage(sessionStorage, EPHEMERAL_PASSWORD_KEY, rndPass);
            await setToStorage(
                sessionStorage,
                EPHEMERAL_VAULT_KEY,
                await this.#vault.encrypt(ephemeralPass)
            );
        });
    }

    public async lock() {
        this.#vault = null;
        await ifSessionStorage(async (sessionStorage) => {
            await setToStorage(sessionStorage, EPHEMERAL_PASSWORD_KEY, null);
            await setToStorage(sessionStorage, EPHEMERAL_VAULT_KEY, null);
        });
    }

    public async revive(): Promise<boolean> {
        let unlocked = false;
        await ifSessionStorage(async (sessionStorage) => {
            const rndPass = await getFromStorage<string>(
                sessionStorage,
                EPHEMERAL_PASSWORD_KEY
            );
            if (rndPass) {
                const ephemeralPass = makeEphemeraPassword(rndPass);
                const ephemeralEncryptedVault =
                    await getFromStorage<StoredData>(
                        sessionStorage,
                        EPHEMERAL_VAULT_KEY
                    );
                if (ephemeralEncryptedVault) {
                    this.#vault = await Vault.from(
                        ephemeralPass,
                        ephemeralEncryptedVault
                    );
                    unlocked = true;
                }
            }
        });
        return unlocked;
    }

    public async clear() {
        await this.lock();
        await setToStorage(LOCAL_STORAGE, VAULT_KEY, null);
    }

    public async isWalletInitialized() {
        return !!(await getFromStorage<StoredData>(LOCAL_STORAGE, VAULT_KEY));
    }

    public getMnemonic() {
        return this.#vault?.getMnemonic() || null;
    }

    public get entropy() {
        return this.#vault?.entropy || null;
    }
}
