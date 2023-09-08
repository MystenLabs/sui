// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type Dispatch,
	type ReactNode,
	createContext,
	useContext,
	useState,
	type SetStateAction,
} from 'react';
import { type FormValues as ImportRecoveryPhraseFormValues } from './ImportRecoveryPhraseForm';
import { type FormValues as ProtectAccountFormValues } from './ProtectAccountForm';

type AccountsFormValues = Partial<ImportRecoveryPhraseFormValues & ProtectAccountFormValues> | null;
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
