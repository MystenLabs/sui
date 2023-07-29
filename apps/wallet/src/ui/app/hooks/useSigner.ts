// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAccounts } from './useAccounts';
import { useActiveAccount } from './useActiveAccount';
import useAppSelector from './useAppSelector';
import { useQredoAPI } from './useQredoAPI';
import { LedgerSigner } from '../LedgerSigner';
import { QredoSigner } from '../QredoSigner';
import { type WalletSigner } from '../WalletSigner';
import { useSuiLedgerClient } from '../components/ledger/SuiLedgerClientProvider';
import { thunkExtras } from '_redux/store/thunk-extras';
import { AccountType } from '_src/background/keyring/Account';

export function useSigner(address?: string): WalletSigner | null {
	const activeAccount = useActiveAccount();
	const existingAccounts = useAccounts();
	const signerAccount = address
		? existingAccounts.find((account) => account.address === address)
		: activeAccount;

	const { connectToLedger } = useSuiLedgerClient();
	const { api, background } = thunkExtras;
	const [qredoAPI] = useQredoAPI(
		signerAccount?.type === AccountType.QREDO ? signerAccount.qredoConnectionID : undefined,
	);
	const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	if (!signerAccount) {
		throw new Error("Can't find account for the signer address");
	}

	if (signerAccount.type === AccountType.LEDGER) {
		return new LedgerSigner(connectToLedger, signerAccount.derivationPath, api.instance.fullNode);
	}
	if (signerAccount.type === AccountType.QREDO) {
		return qredoAPI
			? new QredoSigner(api.instance.fullNode, signerAccount, qredoAPI, networkName)
			: null;
	}
	return api.getSignerInstance(signerAccount, background);
}
