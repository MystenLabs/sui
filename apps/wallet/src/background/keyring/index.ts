// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js';
import mitt from 'mitt';
import { throttle } from 'throttle-debounce';
import Browser from 'webextension-polyfill';

import { Vault } from './Vault';
import { createMessage } from '_messages';
import { isKeyringPayload } from '_payloads/keyring';
import {
    entropyToSerialized,
    getRandomEntropy,
    toEntropy,
} from '_shared/utils/bip39';
import Alarms from '_src/background/Alarms';
import {
    AUTO_LOCK_TIMER_MAX_MINUTES,
    AUTO_LOCK_TIMER_MIN_MINUTES,
    AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

import type { Keypair } from '@mysten/sui.js';
import type { Message } from '_messages';
import type { ErrorPayload } from '_payloads';
import type { KeyringPayload } from '_payloads/keyring';
import type { Connection } from '_src/background/connections/Connection';

type KeyringEvents = {
    lockedStatusUpdate: boolean;
};

const STORAGE_KEY = 'vault';

class Keyring {
    #events = mitt<KeyringEvents>();
    #locked = true;
    #keypair: Keypair | null = null;
    #vault: Vault | null = null;

    /**
     * Creates a vault and stores it encrypted to the storage of the extension. It doesn't unlock the vault.
     * @param password The password to encrypt the vault
     * @param importedEntropy The entropy that was generated from an existing mnemonic that the user provided
     * @throws If the wallet exists or any other error during encrypting/saving to storage or if importedEntropy is invalid
     */
    public async createVault(password: string, importedEntropy?: string) {
        if (await this.isWalletInitialized()) {
            throw new Error(
                'Mnemonic already exists, creating a new one will override it. Clear the existing one first.'
            );
        }
        const vault = new Vault(
            importedEntropy ? toEntropy(importedEntropy) : getRandomEntropy()
        );
        await this.storeEncryptedVault(await vault.encrypt(password));
    }

    public lock() {
        Alarms.clearLockAlarm();
        this.#keypair = null;
        this.#vault = null;
        this.#locked = true;
        this.notifyLockedStatusUpdate(this.#locked);
    }

    public async unlock(password: string) {
        Alarms.setLockAlarm();
        this.#vault = await this.decryptVault(password);
        this.#keypair = Ed25519Keypair.deriveKeypair(this.#vault.getMnemonic());
        this.#locked = false;
        this.notifyLockedStatusUpdate(this.#locked);
    }

    public async clearVault() {
        await this.storeEncryptedVault(null);
        this.lock();
    }

    public async isWalletInitialized() {
        return !!(await this.loadVault());
    }

    public get isLocked() {
        return this.#locked;
    }

    public get keypair() {
        return this.#keypair;
    }

    public get entropy() {
        return this.#vault?.entropy;
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

    public async handleUiMessage(msg: Message, uiConnection: Connection) {
        const { id, payload } = msg;
        try {
            if (
                isKeyringPayload(payload, 'create') &&
                payload.args !== undefined
            ) {
                const { password, importedEntropy } = payload.args;
                await this.createVault(password, importedEntropy);
                await this.unlock(password);
                if (!this.#vault) {
                    throw new Error('Error created vault is empty');
                }
                uiConnection.send(
                    createMessage<KeyringPayload<'create'>>(
                        {
                            type: 'keyring',
                            method: 'create',
                            return: {
                                entropy: entropyToSerialized(
                                    this.#vault.entropy
                                ),
                            },
                        },
                        id
                    )
                );
            } else if (isKeyringPayload(payload, 'getEntropy')) {
                if (this.#locked) {
                    throw new Error('Keyring is locked. Unlock it first.');
                }
                if (!this.#vault) {
                    throw new Error('Error vault is empty');
                }
                uiConnection.send(
                    createMessage<KeyringPayload<'getEntropy'>>(
                        {
                            type: 'keyring',
                            method: 'getEntropy',
                            return: entropyToSerialized(this.#vault.entropy),
                        },
                        id
                    )
                );
            } else if (isKeyringPayload(payload, 'unlock') && payload.args) {
                await this.unlock(payload.args.password);
                uiConnection.send(createMessage({ type: 'done' }, id));
            } else if (isKeyringPayload(payload, 'walletStatusUpdate')) {
                uiConnection.send(
                    createMessage<KeyringPayload<'walletStatusUpdate'>>(
                        {
                            type: 'keyring',
                            method: 'walletStatusUpdate',
                            return: {
                                isLocked: this.isLocked,
                                isInitialized: await this.isWalletInitialized(),
                                entropy: this.#vault?.entropy
                                    ? entropyToSerialized(this.#vault.entropy)
                                    : undefined,
                            },
                        },
                        id
                    )
                );
            } else if (isKeyringPayload(payload, 'lock')) {
                this.lock();
                uiConnection.send(createMessage({ type: 'done' }, id));
            } else if (isKeyringPayload(payload, 'clear')) {
                await this.clearVault();
                uiConnection.send(createMessage({ type: 'done' }, id));
            } else if (isKeyringPayload(payload, 'appStatusUpdate')) {
                const appActive = payload.args?.active;
                if (appActive) {
                    this.postponeLock();
                }
            } else if (isKeyringPayload(payload, 'setLockTimeout')) {
                if (payload.args) {
                    await this.setLockTimeout(payload.args.timeout);
                }
                uiConnection.send(createMessage({ type: 'done' }, id));
            }
        } catch (e) {
            uiConnection.send(
                createMessage<ErrorPayload>(
                    { code: -1, error: true, message: (e as Error).message },
                    id
                )
            );
        }
    }

    // pass null to delete it
    private async storeEncryptedVault(
        encryptedVault: Awaited<ReturnType<Vault['encrypt']>> | null
    ) {
        await Browser.storage.local.set({ [STORAGE_KEY]: encryptedVault });
    }

    private async loadVault() {
        const storedMnemonic = await Browser.storage.local.get({
            [STORAGE_KEY]: null,
        });
        return storedMnemonic[STORAGE_KEY];
    }

    private async decryptVault(password: string) {
        const encryptedVault = await this.loadVault();
        if (!encryptedVault) {
            throw new Error(
                'Mnemonic is not initialized. Create a new one first.'
            );
        }
        return Vault.from(password, encryptedVault, async (aVault) =>
            this.storeEncryptedVault(await aVault.encrypt(password))
        );
    }

    private notifyLockedStatusUpdate(isLocked: boolean) {
        this.#events.emit('lockedStatusUpdate', isLocked);
    }

    private postponeLock = throttle(
        1000,
        async () => {
            if (!this.isLocked) {
                await Alarms.setLockAlarm();
            }
        },
        { noLeading: false }
    );

    private async setLockTimeout(timeout: number) {
        if (
            timeout > AUTO_LOCK_TIMER_MAX_MINUTES ||
            timeout < AUTO_LOCK_TIMER_MIN_MINUTES
        ) {
            return;
        }
        await Browser.storage.local.set({
            [AUTO_LOCK_TIMER_STORAGE_KEY]: timeout,
        });
        if (!this.isLocked) {
            await Alarms.setLockAlarm();
        }
    }
}

export default new Keyring();
