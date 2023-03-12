// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519PublicKey, normalizeSuiAddress } from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import { useSuiLedgerClient } from './SuiLedgerClientProvider';
import { AccountType } from '_src/background/keyring/Account';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';

import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

export type SelectableLedgerAccount = SerializedLedgerAccount & {
    isSelected: boolean;
};

type UseDeriveLedgerAccountOptions = {
    numAccountsToDerive: number;
    onError: (error: unknown) => void;
};

export function useDeriveLedgerAccounts(
    options: UseDeriveLedgerAccountOptions
) {
    const { numAccountsToDerive, onError } = options;
    const [ledgerAccounts, setLedgerAccounts] = useState<
        SelectableLedgerAccount[]
    >([]);
    const [suiLedgerClient] = useSuiLedgerClient();
    const [isLoading, setLoading] = useState(false);

    useEffect(() => {
        const generateLedgerAccounts = async () => {
            setLoading(true);

            try {
                if (!suiLedgerClient) {
                    throw new Error(
                        "The Sui application isn't open on a connected Ledger device"
                    );
                }

                // We have to do this sequentially since Ledger uses a device lock to
                // enure that only one operation is being executed at a time
                const accounts = await deriveAccountsFromLedger(
                    suiLedgerClient,
                    numAccountsToDerive
                );
                setLedgerAccounts(accounts);
            } catch (error) {
                if (onError) {
                    onError(error);
                }
            } finally {
                setLoading(false);
            }
        };
        generateLedgerAccounts();
    }, [numAccountsToDerive, onError, suiLedgerClient]);

    return [ledgerAccounts, setLedgerAccounts, isLoading] as const;
}

async function deriveAccountsFromLedger(
    suiLedgerClient: SuiLedgerClient,
    numAccountsToDerive: number
) {
    const ledgerAccounts: SelectableLedgerAccount[] = [];
    const derivationPaths = getDerivationPathsForLedger(numAccountsToDerive);

    for (const derivationPath of derivationPaths) {
        const publicKeyResult = await suiLedgerClient.getPublicKey(
            derivationPath
        );
        const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
        const suiAddress = normalizeSuiAddress(publicKey.toSuiAddress());
        ledgerAccounts.push({
            type: AccountType.LEDGER,
            address: suiAddress,
            derivationPath,
            isSelected: false,
        });
    }

    return ledgerAccounts;
}

function getDerivationPathsForLedger(numDerivations: number) {
    return Array.from({
        length: numDerivations,
    }).map((_, index) => `m/44'/784'/${index}'/0'/0'`);
}
