// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiLedgerClient } from '../components/ledger/SuiLedgerClientProvider';
import { useAccounts } from './useAccounts';
import { useActiveAccount } from './useActiveAccount';
import { thunkExtras } from '_redux/store/thunk-extras';
import { AccountType } from '_src/background/keyring/Account';

import type { SuiAddress } from '@mysten/sui.js';

export function useSigner(address?: SuiAddress) {
    const activeAccount = useActiveAccount();
    console.log('ACTIVE ACC', activeAccount);
    const existingAccounts = useAccounts();
    const signerAccount = address
        ? existingAccounts.find((account) => account.address === address)
        : activeAccount;

    const [, , getLedgerSignerInstance] = useSuiLedgerClient();
    const { api, background } = thunkExtras;

    if (!signerAccount) {
        throw new Error("Can't find account for the signer address");
    }

    if (signerAccount.type === AccountType.LEDGER) {
        return () => getLedgerSignerInstance(signerAccount.derivationPath);
    }
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore
    return api.getSignerInstance(signerAccount, background, null);
}
