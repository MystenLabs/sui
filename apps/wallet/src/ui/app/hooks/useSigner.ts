// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerSigner } from '../LedgerSigner';
import { useSuiLedgerClient } from '../components/ledger/SuiLedgerClientProvider';
import { useAccounts } from './useAccounts';
import { useActiveAccount } from './useActiveAccount';
import { thunkExtras } from '_redux/store/thunk-extras';
import { AccountType } from '_src/background/keyring/Account';

import type { SuiAddress } from '@mysten/sui.js';

export function useSigner(address?: SuiAddress) {
    const activeAccount = useActiveAccount();
    const existingAccounts = useAccounts();
    const signerAccount = address
        ? existingAccounts.find((account) => account.address === address)
        : activeAccount;

    const { connectToLedger } = useSuiLedgerClient();
    const { api, background } = thunkExtras;

    if (!signerAccount) {
        throw new Error("Can't find account for the signer address");
    }

    if (signerAccount.type === AccountType.LEDGER) {
        return new LedgerSigner(
            connectToLedger,
            signerAccount.derivationPath,
            api.instance.fullNode
        );
    }
    return api.getSignerInstance(signerAccount, background);
}
