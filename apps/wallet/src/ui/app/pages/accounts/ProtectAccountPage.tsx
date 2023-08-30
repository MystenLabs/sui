// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import { ProtectAccountForm } from '../../components/accounts/ProtectAccountForm';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Heading } from '../../shared/heading';
import { Text } from '_app/shared/text';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';

export function ProtectAccountPage() {
	const backgroundClient = useBackgroundClient();
	const [accountsFormValues] = useAccountsFormContext();
	const navigate = useNavigate();
	return (
		<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-6 py-10 h-full">
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
						const mnemonic = accountsFormValues?.recoveryPhrase?.join(' ');
						const accountSource = await backgroundClient.createMnemonicAccountSource({
							password: formValues.password,
							entropy: entropyToSerialized(mnemonicToEntropy(mnemonic!)),
						});
						try {
							await backgroundClient.unlockAccountSourceOrAccount({
								password: formValues.password,
								id: accountSource.id,
							});
							await backgroundClient.createAccounts({
								type: 'mnemonic-derived',
								sourceID: accountSource.id,
							});
						} catch (e) {
							toast.error((e as Error).message ?? 'Failed to create account');
						}
						navigate('/tokens');
					}}
				/>
			</div>
		</div>
	);
}
