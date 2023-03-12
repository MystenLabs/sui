// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';

import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { type SerializedLedgerAccount } from '_src/background/keyring/LedgerAccount';

export function useImportLedgerAccountsMutation() {
    const backgroundClient = useBackgroundClient();
    return useMutation({
        mutationFn: async (ledgerAccounts: SerializedLedgerAccount[]) => {
            return await backgroundClient.importLedgerAccounts(ledgerAccounts);
        },
    });
}
