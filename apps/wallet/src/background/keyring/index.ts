// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js';
import mitt from 'mitt';
import { throttle } from 'throttle-debounce';
import Browser from 'webextension-polyfill';

import { Account } from './Account';
import { VaultStorage } from './VaultStorage';
import { createMessage } from '_messages';
import { isKeyringPayload } from '_payloads/keyring';
import { entropyToSerialized } from '_shared/utils/bip39';
import Alarms from '_src/background/Alarms';
import {
    AUTO_LOCK_TIMER_MAX_MINUTES,
    AUTO_LOCK_TIMER_MIN_MINUTES,
    AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

import type { SuiAddress, ExportedKeypair } from '@mysten/sui.js';
import type { Message } from '_messages';
import type { ErrorPayload } from '_payloads';
import type { KeyringPayload } from '_payloads/keyring';
import type { Connection } from '_src/background/connections/Connection';

/** The key for the extension's storage, that holds the index of the last derived account (zero based) */
const STORAGE_LAST_ACCOUNT_INDEX_KEY = 'last_account_index';
const STORAGE_ACTIVE_ACCOUNT = 'active_account';

type KeyringEvents = {
    lockedStatusUpdate: boolean;
    accountsChanged: Account[];
    activeAccountChanged: string;
};

// exported to make testing easier the default export should be used
export class Keyring {
    #events = mitt<KeyringEvents>();
    #locked = true;
    #vaultStorage: VaultStorage;
    #mainDerivedAccount: SuiAddress | null = null;
    #accountsMap: Map<SuiAddress, Account> = new Map();
    public readonly reviveDone: Promise<void>;

    constructor() {
        this.#vaultStorage = new VaultStorage();
        this.reviveDone = this.revive().catch((e) => {
            // if for some reason decrypting the vault fails or anything else catch
            // the error to allow the user to login using the password
        });
    }

    /**
     * Creates a vault and stores it encrypted to the storage of the extension. It doesn't unlock the vault.
     * @param password The password to encrypt the vault
     * @param importedEntropy The entropy that was generated from an existing mnemonic that the user provided
     * @throws If the wallet exists or any other error during encrypting/saving to storage or if importedEntropy is invalid
     */
    public async createVault(password: string, importedEntropy?: string) {
        await this.#vaultStorage.create(password, importedEntropy);
    }

    public async lock() {
        this.#accountsMap.clear();
        this.#mainDerivedAccount = null;
        this.#locked = true;
        await this.#vaultStorage.lock();
        await Alarms.clearLockAlarm();
        this.notifyLockedStatusUpdate(this.#locked);
    }

    public async unlock(password: string) {
        await this.#vaultStorage.unlock(password);
        await this.unlocked();
    }

    public async clearVault() {
        this.lock();
        await this.#vaultStorage.clear();
    }

    public async isWalletInitialized() {
        return await this.#vaultStorage.isWalletInitialized();
    }

    public get isLocked() {
        return this.#locked;
    }

    public on = this.#events.on;

    public off = this.#events.off;

    public async getActiveAccount() {
        if (this.isLocked) {
            return null;
        }
        const address = (
            await Browser.storage.local.get({
                [STORAGE_ACTIVE_ACCOUNT]: this.#mainDerivedAccount,
            })
        )[STORAGE_ACTIVE_ACCOUNT];
        return this.#accountsMap.get(address) || null;
    }

    public async deriveNextAccount() {
        if (this.isLocked) {
            return false;
        }
        const mnemonic = this.#vaultStorage.getMnemonic();
        if (!mnemonic) {
            return false;
        }
        const nextIndex = (await this.getLastDerivedIndex()) + 1;
        await this.storeLastDerivedIndex(nextIndex);
        const account = this.deriveAccount(nextIndex, mnemonic);
        this.#accountsMap.set(account.address, account);
        this.notifyAccountsChanged();
        return true;
    }

    public getAccounts() {
        if (this.isLocked) {
            return null;
        }
        return Array.from(this.#accountsMap.values());
    }

    public async changeActiveAccount(address: SuiAddress) {
        if (!this.isLocked && this.#accountsMap.has(address)) {
            await this.storeActiveAccount(address);
            this.#events.emit('activeAccountChanged', address);
            return true;
        }
        return false;
    }

    /**
     * Exports the keypair for the specified address. Verifies that the password provided is the correct one and only then returns the keypair.
     * This is useful to be used for exporting the to the UI for the user to backup etc. Getting accounts and keypairs is possible without using
     * a password by using {@link Keypair.getAccounts} or {@link Keypair.getActiveAccount} or the change events
     * @param address The sui address to export the keypair
     * @param password The current password of the vault
     * @returns null if locked or address not found or the exported keypair
     * @throws if wrong password is provided
     */
    public async exportAccountKeypair(address: SuiAddress, password: string) {
        if (this.isLocked) {
            return null;
        }
        if (await this.#vaultStorage.verifyPassword(password)) {
            return this.#accountsMap.get(address)?.exportKeypair() || null;
        } else {
            throw new Error('Wrong password');
        }
    }

    public async importAccountKeypair(
        keypair: ExportedKeypair,
        password: string
    ) {
        if (this.isLocked) {
            // this function is expected to be called from UI when unlocked
            // so this shouldn't happen
            throw new Error('Wallet is locked');
        }
        const passwordCorrect = await this.#vaultStorage.verifyPassword(
            password
        );
        if (!passwordCorrect) {
            // we need to make sure that the password is the same with the one of the current vault because we will
            // update the vault and encrypt it to persist the new keypair in storage
            throw new Error('Wrong password');
        }
        const added = await this.#vaultStorage.importKeypair(keypair, password);
        if (added) {
            this.notifyAccountsChanged();
        }
        return added;
    }

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
                const activeAccount = await this.getActiveAccount();
                if (!activeAccount) {
                    throw new Error('Error created vault is empty');
                }
                uiConnection.send(
                    createMessage<KeyringPayload<'create'>>(
                        {
                            type: 'keyring',
                            method: 'create',
                            return: {
                                keypair: activeAccount.exportKeypair(),
                            },
                        },
                        id
                    )
                );
            } else if (isKeyringPayload(payload, 'getEntropy')) {
                if (this.#locked) {
                    throw new Error('Keyring is locked. Unlock it first.');
                }
                if (!this.#vaultStorage?.entropy) {
                    throw new Error('Error vault is empty');
                }
                uiConnection.send(
                    createMessage<KeyringPayload<'getEntropy'>>(
                        {
                            type: 'keyring',
                            method: 'getEntropy',
                            return: entropyToSerialized(
                                this.#vaultStorage.entropy
                            ),
                        },
                        id
                    )
                );
            } else if (isKeyringPayload(payload, 'unlock') && payload.args) {
                await this.unlock(payload.args.password);
                uiConnection.send(createMessage({ type: 'done' }, id));
            } else if (isKeyringPayload(payload, 'walletStatusUpdate')) {
                // wait to avoid ui showing locked and then unlocked screen
                // ui waits until it receives this status to render
                await this.reviveDone;
                uiConnection.send(
                    createMessage<KeyringPayload<'walletStatusUpdate'>>(
                        {
                            type: 'keyring',
                            method: 'walletStatusUpdate',
                            return: {
                                isLocked: this.isLocked,
                                isInitialized: await this.isWalletInitialized(),
                                activeAccount: (
                                    await this.getActiveAccount()
                                )?.exportKeypair(),
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

    private async revive() {
        const unlocked = await this.#vaultStorage.revive();
        if (unlocked) {
            await this.unlocked();
        }
    }

    private async unlocked() {
        let mnemonic = this.#vaultStorage.getMnemonic();
        if (!mnemonic) {
            return;
        }
        Alarms.setLockAlarm();
        const lastAccountIndex = await this.getLastDerivedIndex();
        for (let i = 0; i <= lastAccountIndex; i++) {
            const account = this.deriveAccount(i, mnemonic);
            this.#accountsMap.set(account.address, account);
            if (i === 0) {
                this.#mainDerivedAccount = account.address;
            }
        }
        this.#vaultStorage.getImportedKeys()?.forEach((anImportedKey) => {
            const account = new Account('imported', { keypair: anImportedKey });
            this.#accountsMap.set(account.address, account);
        });
        mnemonic = null;
        this.#locked = false;
        this.notifyLockedStatusUpdate(this.#locked);
    }

    private deriveAccount(accountIndex: number, mnemonic: string) {
        const derivationPath = this.makeDerivationPath(accountIndex);
        const keypair = Ed25519Keypair.deriveKeypair(mnemonic, derivationPath);
        return new Account('derived', { keypair, derivationPath });
    }

    private async getLastDerivedIndex() {
        return (
            await Browser.storage.local.get({
                [STORAGE_LAST_ACCOUNT_INDEX_KEY]: 0,
            })
        )[STORAGE_LAST_ACCOUNT_INDEX_KEY] as number;
    }

    private storeLastDerivedIndex(index: number) {
        return Browser.storage.local.set({
            [STORAGE_LAST_ACCOUNT_INDEX_KEY]: index,
        });
    }

    private storeActiveAccount(address: SuiAddress) {
        return Browser.storage.local.set({ [STORAGE_ACTIVE_ACCOUNT]: address });
    }

    private makeDerivationPath(index: number) {
        // currently returns only Ed25519 path
        return `m/44'/784'/${index}'/0'/0'`;
    }

    private notifyAccountsChanged() {
        this.#events.emit('accountsChanged', this.getAccounts() || []);
    }
}

export default new Keyring();
