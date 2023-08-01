// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerLogo17 as LedgerLogo } from '@mysten/icons';
import { useState, type ReactNode } from 'react';
import toast from 'react-hot-toast';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { ConnectLedgerModal } from '../../components/ledger/ConnectLedgerModal';
import { getLedgerConnectionErrorMessage } from '../../helpers/errorMessages';
import { SocialButton } from '../../shared/SocialButton';
import { Button } from '_app/shared/ButtonUI';
import { Text } from '_app/shared/text';
import Overlay from '_components/overlay';
import { ampli } from '_src/shared/analytics/ampli';

type AddAccountPageProps = {
	showSocialSignInOptions?: boolean;
};

export function AddAccountPage({ showSocialSignInOptions = false }: AddAccountPageProps) {
	const [isConnectLedgerModalOpen, setConnectLedgerModalOpen] = useState(false);
	const [searchParams] = useSearchParams();
	const navigate = useNavigate();

	const sourceFlow = searchParams.get('sourceFlow') || 'Unknown';

	return (
		<Overlay showModal title="Add Account" closeOverlay={() => navigate('/')}>
			<div className="w-full flex flex-col gap-8 pt-3">
				<div className="flex flex-col gap-3">
					{showSocialSignInOptions && (
						<>
							<SocialButton
								provider="google"
								showLabel
								onClick={() => {
									// eslint-disable-next-line no-console
									console.log('TODO: Open OAuth flow');
									ampli.clickedSocialSignInButton({
										signInProvider: 'Google',
										sourceFlow,
									});
								}}
							/>
							<SocialButton
								provider="twitch"
								showLabel
								onClick={() => {
									// eslint-disable-next-line no-console
									console.log('TODO: Open OAuth flow');
									ampli.clickedSocialSignInButton({
										signInProvider: 'Twitch',
										sourceFlow,
									});
								}}
							/>
							<SocialButton
								provider="facebook"
								showLabel
								onClick={() => {
									// eslint-disable-next-line no-console
									console.log('TODO: Open OAuth flow');
									ampli.clickedSocialSignInButton({
										signInProvider: 'Facebook',
										sourceFlow,
									});
								}}
							/>
							<SocialButton
								provider="microsoft"
								showLabel
								onClick={() => {
									// eslint-disable-next-line no-console
									console.log('TODO: Open OAuth flow');
									ampli.clickedSocialSignInButton({
										signInProvider: 'Microsoft',
										sourceFlow,
									});
								}}
							/>
						</>
					)}
					<Button
						variant="outline"
						size="tall"
						text="Set up Ledger"
						before={<LedgerLogo className="text-gray-90" width={16} height={16} />}
						onClick={() => {
							setConnectLedgerModalOpen(true);
							ampli.openedConnectLedgerFlow({ sourceFlow });
						}}
					/>
				</div>
				<Section title="Create New">
					<Button
						variant="outline"
						size="tall"
						text="Create a new Passphrase Account"
						to="/accounts/protect-account"
						onClick={() => {
							ampli.clickedCreateNewAccount({ sourceFlow });
						}}
					/>
				</Section>
				<Section title="Import Existing Accounts">
					<Button
						variant="outline"
						size="tall"
						text="Import Passphrase"
						to="/accounts/import-passphrase"
						onClick={() => {
							ampli.clickedImportPassphrase({ sourceFlow });
						}}
					/>
					<Button
						variant="outline"
						size="tall"
						text="Import Private Key"
						to="/accounts/import-private-key"
						onClick={() => {
							ampli.clickedImportPrivateKey({ sourceFlow });
						}}
					/>
				</Section>
			</div>
			{isConnectLedgerModalOpen && (
				<ConnectLedgerModal
					onClose={() => {
						setConnectLedgerModalOpen(false);
					}}
					onError={(error) => {
						setConnectLedgerModalOpen(false);
						toast.error(getLedgerConnectionErrorMessage(error) || 'Something went wrong.');
					}}
					onConfirm={() => {
						ampli.connectedHardwareWallet({ hardwareWalletType: 'Ledger' });
						navigate('/accounts/import-ledger-accounts');
					}}
				/>
			)}
		</Overlay>
	);
}

type SectionProps = {
	title: string;
	children: ReactNode;
};

function Section({ title, children }: SectionProps) {
	return (
		<section className="flex flex-col gap-3">
			<div className="flex items-center gap-2">
				<div className="grow border-0 border-t border-solid border-gray-40"></div>
				<Text variant="caption" weight="semibold" color="steel">
					{title}
				</Text>
				<div className="grow border-0 border-t border-solid border-gray-40"></div>
			</div>
			{children}
		</section>
	);
}
