// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519PublicKey } from '@mysten/sui.js';
import { useEffect, useState } from 'react';

import { type LedgerAccount } from './LedgerAccountItem';
import { useSuiLedgerClient } from './SuiLedgerClientProvider';

import type SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

type UseDeriveLedgerAccountOptions = {
    numAccountsToDerive: number;
    onError: (error: unknown) => void;
};

type UseDeriveLedgerAccountResult = [
    LedgerAccount[],
    React.Dispatch<React.SetStateAction<LedgerAccount[]>>,
    boolean
];

export function useDeriveLedgerAccounts(
    options: UseDeriveLedgerAccountOptions
): UseDeriveLedgerAccountResult {
    const { numAccountsToDerive, onError } = options;
    const [ledgerAccounts, setLedgerAccounts] = useState<LedgerAccount[]>([]);
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

    return [ledgerAccounts, setLedgerAccounts, isLoading];
}

async function deriveAccountsFromLedger(
    suiLedgerClient: SuiLedgerClient,
    numAccountsToDerive: number
) {
    const ledgerAccounts: LedgerAccount[] = [];
    const derivationPaths = getDerivationPathsForLedger(numAccountsToDerive);

    for (const derivationPath of derivationPaths) {
        const publicKeyResult = await suiLedgerClient.getPublicKey(
            derivationPath
        );
        const publicKey = new Ed25519PublicKey(publicKeyResult.publicKey);
        ledgerAccounts.push({
            isSelected: false,
            address: publicKey.toSuiAddress(),
        });
    }

    return ledgerAccounts;
}

function getDerivationPathsForLedger(numDerivations: number) {
    return Array.from({
        length: numDerivations,
    }).map((_, index) => `m/44'/784'/${index}'/0'/0'`);
}
