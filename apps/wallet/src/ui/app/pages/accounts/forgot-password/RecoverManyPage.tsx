// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';
import { ImportRecoveryPhraseForm } from '_src/ui/app/components/accounts/ImportRecoveryPhraseForm';
import Overlay from '_src/ui/app/components/overlay';
import { useRecoveryDataMutation } from '_src/ui/app/hooks/useRecoveryDataMutation';
import { useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';

import { RecoverAccountsGroup } from '../../../components/accounts/RecoverAccountsGroup';
import { useAccountGroups } from '../../../hooks/useAccountGroups';
import { useAccountSources } from '../../../hooks/useAccountSources';
import { Button } from '../../../shared/ButtonUI';
import { Heading } from '../../../shared/heading';
import { Text } from '../../../shared/text';
import { useForgotPasswordContext } from './ForgotPasswordPage';

export function RecoverManyPage() {
	const allAccountSources = useAccountSources();
	const accountGroups = useAccountGroups();
	const navigate = useNavigate();
	useEffect(() => {
		if (
			!allAccountSources.isPending &&
			!allAccountSources.data?.find(({ type }) => type === 'mnemonic')
		) {
			navigate('/', { replace: true });
		}
	}, [allAccountSources.isPending, allAccountSources.data, navigate]);
	const { value } = useForgotPasswordContext();
	const addRecoveryDataMutation = useRecoveryDataMutation();
	const [recoverInfo, setRecoverInfo] = useState<{ title: string; accountSourceID: string } | null>(
		null,
	);
	return (
		<>
			<div className="flex flex-col items-center h-full gap-6">
				<div className="flex flex-col items-center gap-2 text-center">
					<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
						Forgot Password?
					</Heading>
					<Text variant="pBody" color="gray-90">
						Please complete the recovery process for the accounts below
					</Text>
				</div>
				<div className="flex flex-col grow self-stretch overflow-x-hidden overflow-y-auto gap-8 px-4 py-6 rounded-lg bg-hero-darkest/5">
					{Object.entries(accountGroups['mnemonic-derived']).map(([sourceID, accounts], index) => {
						const recoveryData = value.find(({ accountSourceID }) => accountSourceID === sourceID);
						const title = `Passphrase ${index + 1}`;
						return (
							<RecoverAccountsGroup
								key={sourceID}
								title={title}
								accounts={accounts}
								showRecover={!recoveryData}
								onRecover={() => {
									setRecoverInfo({ title, accountSourceID: sourceID });
								}}
								recoverDone={!!recoveryData}
							/>
						);
					})}
				</div>
				<div className="flex flex-nowrap gap-2.5 w-full">
					<Button variant="outline" size="tall" text="Cancel" to="/" />
					<Button
						variant="primary"
						size="tall"
						text="Next"
						disabled={!value.length}
						to="../warning"
					/>
				</div>
			</div>
			<Overlay
				title={recoverInfo?.title}
				showModal={!!recoverInfo}
				closeOverlay={() => {
					if (addRecoveryDataMutation.isPending) {
						return;
					}
					setRecoverInfo(null);
				}}
				background="bg-sui-lightest"
			>
				<div className="flex flex-col flex-nowrap w-full h-full gap-4 text-center">
					<Text variant="pBody" color="gray-90">
						Enter your 12-word Recovery Phrase
					</Text>
					<ImportRecoveryPhraseForm
						submitButtonText="Recover"
						onSubmit={async ({ recoveryPhrase }) => {
							if (!recoverInfo) {
								return;
							}
							try {
								await addRecoveryDataMutation.mutateAsync({
									type: 'mnemonic',
									entropy: entropyToSerialized(mnemonicToEntropy(recoveryPhrase.join(' '))),
									accountSourceID: recoverInfo.accountSourceID,
								});
								setRecoverInfo(null);
							} catch (e) {
								toast.error((e as Error)?.message || 'Something went wrong');
							}
						}}
					/>
				</div>
			</Overlay>
		</>
	);
}
