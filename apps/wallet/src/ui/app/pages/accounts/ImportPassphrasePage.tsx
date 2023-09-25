// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';
import { useNavigate } from 'react-router-dom';

import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import { ImportRecoveryPhraseForm } from '../../components/accounts/ImportRecoveryPhraseForm';
import { Heading } from '../../shared/heading';

export function ImportPassphrasePage() {
	const navigate = useNavigate();
	const [, setFormValues] = useAccountsFormContext();
	return (
		<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-6 py-10 h-full overflow-auto">
			<Text variant="caption" color="steel-dark" weight="semibold">
				Wallet Setup
			</Text>
			<div className="text-center mt-2.5">
				<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
					Add Existing Account
				</Heading>
			</div>
			<div className="mt-6 grow flex flex-col gap-3">
				<div className="pl-2.5">
					<Text variant="pBody" color="steel-darker" weight="semibold">
						Enter your 12-word Recovery Phrase
					</Text>
				</div>
				<ImportRecoveryPhraseForm
					cancelButtonText="Cancel"
					submitButtonText="Add Account"
					onSubmit={({ recoveryPhrase }) => {
						setFormValues({
							type: 'import-mnemonic',
							entropy: entropyToSerialized(mnemonicToEntropy(recoveryPhrase.join(' '))),
						});
						navigate('/accounts/protect-account?accountType=import-mnemonic');
					}}
				/>
			</div>
		</div>
	);
}
