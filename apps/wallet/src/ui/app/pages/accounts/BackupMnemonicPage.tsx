// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_app/shared/ButtonUI';
import { CardLayout } from '_app/shared/card-layout';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { HideShowDisplayBox } from '_src/ui/app/components/HideShowDisplayBox';
import { ArrowLeft16, Check12 } from '@mysten/icons';
import { useEffect, useMemo, useState } from 'react';
import { Navigate, useLocation, useNavigate, useParams } from 'react-router-dom';

import { VerifyPasswordModal } from '../../components/accounts/VerifyPasswordModal';
import { useAccountSources } from '../../hooks/useAccountSources';
import { useExportPassphraseMutation } from '../../hooks/useExportPassphraseMutation';

export function BackupMnemonicPage() {
	const [passwordCopied, setPasswordCopied] = useState(false);
	const { state } = useLocation();
	const { accountSourceID } = useParams();
	const { data: accountSources, isPending } = useAccountSources();
	const selectedSource = useMemo(
		() => accountSources?.find(({ id }) => accountSourceID === id),
		[accountSources, accountSourceID],
	);
	const isOnboardingFlow = !!state?.onboarding;
	const [showPasswordDialog, setShowPasswordDialog] = useState(false);
	const [passwordConfirmed, setPasswordConfirmed] = useState(false);
	const requirePassword = !isOnboardingFlow || !!selectedSource?.isLocked;
	const passphraseMutation = useExportPassphraseMutation();
	useEffect(() => {
		(async () => {
			if (
				(requirePassword && !passwordConfirmed) ||
				!passphraseMutation.isIdle ||
				!accountSourceID
			) {
				return;
			}
			passphraseMutation.mutate({ accountSourceID: accountSourceID });
		})();
	}, [requirePassword, passwordConfirmed, accountSourceID, passphraseMutation]);
	useEffect(() => {
		if (requirePassword && !passwordConfirmed && !showPasswordDialog) {
			setShowPasswordDialog(true);
		}
	}, [requirePassword, passwordConfirmed, showPasswordDialog]);
	const navigate = useNavigate();
	if (!isPending && selectedSource?.type !== 'mnemonic') {
		return <Navigate to="/" replace />;
	}
	return (
		<Loading loading={isPending}>
			{showPasswordDialog ? (
				<CardLayout>
					<VerifyPasswordModal
						open
						onClose={() => {
							navigate(-1);
						}}
						onVerify={async (password) => {
							await passphraseMutation.mutateAsync({
								password,
								accountSourceID: selectedSource!.id,
							});
							setPasswordConfirmed(true);
							setShowPasswordDialog(false);
						}}
					/>
				</CardLayout>
			) : (
				<CardLayout
					icon={isOnboardingFlow ? 'success' : undefined}
					title={isOnboardingFlow ? 'Wallet Created Successfully!' : 'Backup Recovery Phrase'}
				>
					<div className="flex flex-col flex-nowrap flex-grow h-full w-full">
						<div className="flex flex-col flex-nowrap flex-grow mb-5">
							<div className="mb-1 mt-7.5 text-center">
								<Text variant="caption" color="steel-darker" weight="bold">
									Recovery phrase
								</Text>
							</div>
							<div className="mb-3.5 mt-2 text-center">
								<Text variant="pBodySmall" color="steel-dark" weight="normal">
									Your recovery phrase makes it easy to back up and restore your account.
								</Text>
							</div>
							<Loading loading={passphraseMutation.isPending}>
								{passphraseMutation.data ? (
									<HideShowDisplayBox value={passphraseMutation.data} hideCopy />
								) : (
									<Alert>
										{(passphraseMutation.error as Error)?.message || 'Something went wrong'}
									</Alert>
								)}
							</Loading>
							<div className="mt-3.75 mb-1 text-center">
								<Text variant="caption" color="steel-dark" weight="semibold">
									Warning
								</Text>
							</div>
							<div className="mb-1 text-center">
								<Text variant="pBodySmall" color="steel-dark" weight="normal">
									Never disclose your secret recovery phrase. Anyone can take over your account with
									it.
								</Text>
							</div>
							<div className="flex-1" />
							{isOnboardingFlow ? (
								<div className="w-full text-left flex mt-5 mb-">
									<label className="flex items-center justify-center h-5 mb-0 mr-5 text-sui-dark gap-1.25 relative cursor-pointer">
										<input
											type="checkbox"
											name="agree"
											id="agree"
											className="peer/agree invisible ml-2"
											onChange={() => setPasswordCopied(!passwordCopied)}
										/>
										<span className="absolute top-0 left-0 h-5 w-5 bg-white peer-checked/agree:bg-success peer-checked/agree:shadow-none border-gray-50 border rounded shadow-button flex justify-center items-center">
											<Check12 className="text-white text-body font-semibold" />
										</span>

										<Text variant="bodySmall" color="steel-dark" weight="normal">
											I saved my recovery phrase
										</Text>
									</label>
								</div>
							) : null}
						</div>
						<Button
							type="button"
							size="tall"
							variant="primary"
							disabled={!passwordCopied && isOnboardingFlow}
							to="/"
							text="Open Sui Wallet"
							after={<ArrowLeft16 className="text-pBodySmall font-normal rotate-135" />}
						/>
					</div>
				</CardLayout>
			)}
		</Loading>
	);
}
