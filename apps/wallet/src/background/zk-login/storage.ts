import { type SuiAddress } from '@mysten/sui.js';
import Browser from 'webextension-polyfill';

export type StoredZkLoginAccount = {
    address: SuiAddress;
    pin: string;
    sub: string;
    email: string;
};

export function storeZkLoginAccount(account: StoredZkLoginAccount) {
    return Browser.storage.local.set({ zkLoginAccount: account });
}

export async function getStoredZkLoginAccount(): Promise<StoredZkLoginAccount | null> {
    return (await Browser.storage.local.get({ zkLoginAccount: null }))
        .zkLoginAccount;
}
