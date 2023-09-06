// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair } from '@mysten/sui.js/cryptography';
import {
	type Dispatch,
	type ReactNode,
	createContext,
	useContext,
	useState,
	type SetStateAction,
} from 'react';
import { type ZkProvider } from '_src/background/accounts/zk/providers';

export type AccountsFormValues =
	| { type: 'zk'; provider: ZkProvider }
	| { type: 'new-mnemonic' }
	| { type: 'import-mnemonic'; entropy: string }
	| { type: 'mnemonic-derived'; sourceID: string }
	| { type: 'imported'; keyPair: ExportedKeypair }
	| {
			type: 'ledger';
			accounts: { publicKey: string; derivationPath: string; address: string }[];
	  }
	| null;

type AccountsFormContextType = [AccountsFormValues, Dispatch<SetStateAction<AccountsFormValues>>];

const AccountsFormContext = createContext<AccountsFormContextType | null>(null);

export const AccountsFormProvider = ({ children }: { children: ReactNode }) => {
	const value = useState<AccountsFormValues>(null);
	return <AccountsFormContext.Provider value={value}>{children}</AccountsFormContext.Provider>;
};

// a simple hook that allows form values to be shared between forms when setting up an account
// for the first time, or when importing an existing account.
export const useAccountsFormContext = () => {
	const context = useContext(AccountsFormContext);
	if (!context) {
		throw new Error('useAccountsFormContext must be used within the AccountsFormProvider');
	}
	return context;
};
