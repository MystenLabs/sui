import {
    type ExportedKeypair,
    type Keypair,
    type SuiAddress,
    fromExportedKeypair,
} from '@mysten/sui.js';

import { type ZkProofsResponse } from '.';
import {
    getRandomPassword,
    makeEphemeraPassword,
} from '../keyring/VaultStorage';
import { getFromSessionStorage, setToSessionStorage } from '../storage-utils';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';

type StoredAccount = {
    random: string;
    data: string;
};
type StoredAccountData = {
    address: SuiAddress;
    proofs: ZkProofsResponse;
    ephemeralKeyPair: ExportedKeypair;
};

function accountSessionKey(address: SuiAddress) {
    return `data_zk_${address}`;
}

export async function cacheAccountCredentials({
    address,
    proofs,
    ephemeralKeyPair,
}: {
    address: SuiAddress;
    ephemeralKeyPair: Keypair;
    proofs: ZkProofsResponse;
}) {
    const random = getRandomPassword();
    const accountData: StoredAccountData = {
        address,
        proofs,
        ephemeralKeyPair: ephemeralKeyPair.export(),
    };
    await setToSessionStorage<StoredAccount>(accountSessionKey(address), {
        random,
        data: await encrypt(makeEphemeraPassword(random), accountData),
    });
}

export async function getCachedAccountCredentials(address: SuiAddress) {
    const storedAccount = await getFromSessionStorage<StoredAccount>(
        accountSessionKey(address),
        null
    );
    if (storedAccount) {
        const accountData = await decrypt<StoredAccountData>(
            makeEphemeraPassword(storedAccount.random),
            storedAccount.data
        );
        return {
            ...accountData,
            ephemeralKeyPair: fromExportedKeypair(accountData.ephemeralKeyPair),
        };
    }
    return null;
}
