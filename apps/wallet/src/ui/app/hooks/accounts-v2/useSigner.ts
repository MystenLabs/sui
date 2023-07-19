// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { LedgerSigner } from '../../LedgerSigner';
import { QredoSigner } from '../../QredoSigner';
import { type WalletSigner } from '../../WalletSigner';
import { useSuiLedgerClient } from '../../components/ledger/SuiLedgerClientProvider';
import { thunkExtras } from '../../redux/store/thunk-extras';
import useAppSelector from '../useAppSelector';
import { useBackgroundClient } from '../useBackgroundClient';
import { useQredoAPI } from '../useQredoAPI';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { type SerializedQredoAccount } from '_src/background/keyring/QredoAccount';

export function useSigner(account: SerializedUIAccount | null): WalletSigner | null {
	const { connectToLedger } = useSuiLedgerClient();
	const api = useRpcClient();
	const background = useBackgroundClient();
	const [qredoAPI] = useQredoAPI(account?.type === 'qredo' ? undefined /* TODO */ : undefined);
	const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	if (!account) {
		return null;
	}
	if (account.type === 'ledger') {
		return new LedgerSigner(connectToLedger, 'account.derivationPath' /* TODO */, api);
	}
	if (account.type === 'qredo') {
		return qredoAPI
			? new QredoSigner(
					api,
					account as unknown as SerializedQredoAccount /* TODO */,
					qredoAPI,
					networkName,
			  )
			: null;
	}
	return thunkExtras.api.getSignerInstance(account, background);
}
