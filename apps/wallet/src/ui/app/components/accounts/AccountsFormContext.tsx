// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ZkLoginProvider } from '_src/background/accounts/zklogin/providers';
import { type Wallet } from '_src/shared/qredo-api';
import {
	createContext,
	useCallback,
	useContext,
	useMemo,
	useRef,
	type MutableRefObject,
	type ReactNode,
} from 'react';

export type AccountsFormValues =
	| { type: 'zkLogin'; provider: ZkLoginProvider }
	| { type: 'new-mnemonic' }
	| { type: 'import-mnemonic'; entropy: string }
	| { type: 'mnemonic-derived'; sourceID: string }
	| { type: 'imported'; keyPair: string }
	| {
			type: 'ledger';
			accounts: { publicKey: string; derivationPath: string; address: string }[];
	  }
	| { type: 'qredo'; accounts: Wallet[]; qredoID: string }
	| null;

type AccountsFormContextType = [
	MutableRefObject<AccountsFormValues>,
	(values: AccountsFormValues) => void,
];

const AccountsFormContext = createContext<AccountsFormContextType | null>(null);

export const AccountsFormProvider = ({ children }: { children: ReactNode }) => {
	const valuesRef = useRef<AccountsFormValues>(null);
	const setter = useCallback((values: AccountsFormValues) => {
		valuesRef.current = values;
	}, []);
	const value = useMemo(() => [valuesRef, setter] as AccountsFormContextType, [setter]);
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
