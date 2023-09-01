// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useBackgroundClient } from './useBackgroundClient';
import {
	type AccountsFormValues,
	useAccountsFormContext,
} from '../components/accounts/AccountsFormContext';

export type CreateType = NonNullable<AccountsFormValues>['type'];

function validateAccountFormValues<T extends CreateType>(
	createType: T,
	values: AccountsFormValues,
): values is Extract<AccountsFormValues, { type: T }> {
	if (!values) {
		throw new Error('Missing account data values');
	}
	if (values.type !== createType) {
		throw new Error('Account data values type mismatch');
	}
	if (values.type !== 'zk' && !values.password) {
		throw new Error('Missing password');
	}
	return true;
}

export function useCreateAccountsMutation() {
	const backgroundClient = useBackgroundClient();
	const [accountsFormValues, setAccountFormValues] = useAccountsFormContext();
	console.log(accountsFormValues);
	return useMutation({
		mutationKey: ['create accounts'],
		mutationFn: async ({ type }: { type: CreateType }) => {
			let createdAccounts;
			if (type === 'zk' && validateAccountFormValues(type, accountsFormValues)) {
				createdAccounts = await backgroundClient.createAccounts(accountsFormValues);
			} else if (
				(type === 'new-mnemonic' || type === 'import-mnemonic') &&
				validateAccountFormValues(type, accountsFormValues)
			) {
				// validateAccountFormValues checks the password
				const password = accountsFormValues.password!;
				const accountSource = await backgroundClient.createMnemonicAccountSource({
					password,
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
				validateAccountFormValues(type, accountsFormValues)
			) {
				// validateAccountFormValues checks the password
				const password = accountsFormValues.password!;
				await backgroundClient.unlockAccountSourceOrAccount({
					password,
					id: accountsFormValues.sourceID,
				});
				createdAccounts = await backgroundClient.createAccounts({
					type: 'mnemonic-derived',
					sourceID: accountsFormValues.sourceID,
				});
				// TODO implement all types
			} else {
				throw new Error('Not implemented yet');
			}
			if (createdAccounts) {
				for (const aCreatedAccount of createdAccounts) {
					await backgroundClient.unlockAccountSourceOrAccount({
						id: aCreatedAccount.id,
						password: 'password' in accountsFormValues ? accountsFormValues.password : undefined,
					});
				}
			}
			setAccountFormValues(null);
			return createdAccounts;
		},
	});
}
