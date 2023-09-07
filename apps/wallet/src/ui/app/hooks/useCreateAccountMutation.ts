// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';
import {
	type AccountsFormValues,
	useAccountsFormContext,
} from '../components/accounts/AccountsFormContext';
import { type AddedAccountsProperties, ampli } from '_src/shared/analytics/ampli';

export type CreateType = NonNullable<AccountsFormValues>['type'];

function validateAccountFormValues<T extends CreateType>(
	createType: T,
	values: AccountsFormValues,
	password?: string,
): values is Extract<AccountsFormValues, { type: T }> {
	if (!values) {
		throw new Error('Missing account data values');
	}
	if (values.type !== createType) {
		throw new Error('Account data values type mismatch');
	}
	if (values.type !== 'zk' && !password) {
		throw new Error('Missing password');
	}
	return true;
}

const createTypeToAmpliAccount: Record<CreateType, AddedAccountsProperties['accountType']> = {
	zk: 'Zklogin',
	'new-mnemonic': 'Derived',
	'import-mnemonic': 'Derived',
	'mnemonic-derived': 'Derived',
	imported: 'Imported',
	ledger: 'Ledger',
	qredo: 'Qredo',
};

export function useCreateAccountsMutation() {
	const backgroundClient = useBackgroundClient();
	const [accountsFormValuesRef, setAccountFormValues] = useAccountsFormContext();
	return useMutation({
		mutationKey: ['create accounts'],
		mutationFn: async ({ type, password }: { type: CreateType; password?: string }) => {
			let createdAccounts;
			const accountsFormValues = accountsFormValuesRef.current;
			if (type === 'zk' && validateAccountFormValues(type, accountsFormValues)) {
				createdAccounts = await backgroundClient.createAccounts(accountsFormValues);
			} else if (
				(type === 'new-mnemonic' || type === 'import-mnemonic') &&
				validateAccountFormValues(type, accountsFormValues, password)
			) {
				const accountSource = await backgroundClient.createMnemonicAccountSource({
					// validateAccountFormValues checks the password
					password: password!,
					entropy: 'entropy' in accountsFormValues ? accountsFormValues.entropy : undefined,
				});
				await backgroundClient.unlockAccountSourceOrAccount({
					password,
					id: accountSource.id,
				});
				createdAccounts = await backgroundClient.createAccounts({
					type: 'mnemonic-derived',
					sourceID: accountSource.id,
				});
			} else if (
				type === 'mnemonic-derived' &&
				validateAccountFormValues(type, accountsFormValues, password)
			) {
				await backgroundClient.unlockAccountSourceOrAccount({
					password,
					id: accountsFormValues.sourceID,
				});
				createdAccounts = await backgroundClient.createAccounts({
					type: 'mnemonic-derived',
					sourceID: accountsFormValues.sourceID,
				});
			} else if (
				type === 'imported' &&
				validateAccountFormValues(type, accountsFormValues, password)
			) {
				createdAccounts = await backgroundClient.createAccounts({
					type: 'imported',
					keyPair: accountsFormValues.keyPair,
					password: password!,
				});
			} else if (
				type === 'ledger' &&
				validateAccountFormValues(type, accountsFormValues, password)
			) {
				createdAccounts = await backgroundClient.createAccounts({
					type: 'ledger',
					accounts: accountsFormValues.accounts,
					password: password!,
				});
			} else if (
				type === 'qredo' &&
				validateAccountFormValues(type, accountsFormValues, password)
			) {
				createdAccounts = await backgroundClient.acceptQredoConnection({
					qredoID: accountsFormValues.qredoID,
					accounts: accountsFormValues.accounts,
					password: password!,
				});
			} else {
				throw new Error(`Create accounts with type ${type} is not implemented yet`);
			}
			for (const aCreatedAccount of createdAccounts) {
				await backgroundClient.unlockAccountSourceOrAccount({
					id: aCreatedAccount.id,
					password,
				});
			}
			ampli.addedAccounts({
				accountType: createTypeToAmpliAccount[type],
				numberOfAccounts: createdAccounts.length,
			});
			setAccountFormValues(null);
			return createdAccounts;
		},
	});
}
