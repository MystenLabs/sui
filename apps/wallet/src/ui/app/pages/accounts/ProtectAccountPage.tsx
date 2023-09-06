// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { ProtectAccountForm } from '../../components/accounts/ProtectAccountForm';
import { useAccounts } from '../../hooks/useAccounts';
import { type CreateType, useCreateAccountsMutation } from '../../hooks/useCreateAccountMutation';
import { Heading } from '../../shared/heading';
import { Text } from '_app/shared/text';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';

const allowedAccountTypes: CreateType[] = [
	'new-mnemonic',
	'import-mnemonic',
	'mnemonic-derived',
	'imported',
	'ledger',
	'qredo',
];

type AllowedAccountTypes = (typeof allowedAccountTypes)[number];

function isAllowedAccountType(accountType: string): accountType is AllowedAccountTypes {
	return allowedAccountTypes.includes(accountType as any);
}

export function ProtectAccountPage() {
	const [searchParams] = useSearchParams();
	const accountType = searchParams.get('accountType') || '';
	const successRedirect = searchParams.get('successRedirect') || '/tokens';
	const navigate = useNavigate();
	const { data: accounts } = useAccounts();
	const createMutation = useCreateAccountsMutation();
	useEffect(() => {
		// don't show this page if other password accounts exist (we should show the verify password instead)
		if (
			accounts?.length &&
			accounts.some(({ isPasswordUnlockable }) => isPasswordUnlockable) &&
			!createMutation.isLoading &&
			!createMutation.isSuccess
		) {
			navigate('/', { replace: true });
		}
	}, [accounts, navigate, createMutation.isSuccess, createMutation.isLoading]);
	if (!isAllowedAccountType(accountType)) {
		return <Navigate to="/" replace />;
	}
	return (
		<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-6 py-10 h-full overflow-auto">
			<Text variant="caption" color="steel-dark" weight="semibold">
				Wallet Setup
			</Text>
			<div className="text-center mt-2.5">
				<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
					Protect Account with a Password Lock
				</Heading>
			</div>
			<div className="mt-6 w-full grow">
				<ProtectAccountForm
					cancelButtonText="Back"
					submitButtonText="Create Wallet"
					onSubmit={async (formValues) => {
						try {
							const createdAccounts = await createMutation.mutateAsync({
								type: accountType,
								password: formValues.password.input,
							});
							if (
								accountType === 'new-mnemonic' &&
								isMnemonicSerializedUiAccount(createdAccounts[0])
							) {
								navigate(`/accounts/backup/${createdAccounts[0].sourceID}`, {
									replace: true,
									state: {
										onboarding: true,
									},
								});
							} else {
								navigate(successRedirect, { replace: true });
							}
						} catch (e) {
							toast.error((e as Error).message ?? 'Failed to create account');
						}
					}}
				/>
			</div>
		</div>
	);
}
