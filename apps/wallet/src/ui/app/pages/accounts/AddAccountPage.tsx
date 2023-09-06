// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerLogo17 as LedgerLogo } from '@mysten/icons';
import { useState, type ReactNode } from 'react';
import toast from 'react-hot-toast';
import { useNavigate, useSearchParams } from 'react-router-dom';
import Browser from 'webextension-polyfill';
import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import { ConnectLedgerModal } from '../../components/ledger/ConnectLedgerModal';
import { getLedgerConnectionErrorMessage } from '../../helpers/errorMessages';
import { useAppSelector } from '../../hooks';
import { AppType } from '../../redux/slices/app/AppType';
import { SocialButton } from '../../shared/SocialButton';
import { Button } from '_app/shared/ButtonUI';
import { Text } from '_app/shared/text';
import Overlay from '_components/overlay';
import { ampli } from '_src/shared/analytics/ampli';

export function AddAccountPage() {
	const [searchParams] = useSearchParams();
	const navigate = useNavigate();
	const sourceFlow = searchParams.get('sourceFlow') || 'Unknown';
	const showSocialSignInOptions = sourceFlow !== 'Onboarding';
	const forceShowLedger =
		searchParams.has('showLedger') && searchParams.get('showLedger') !== 'false';
	const [, setAccountFormValues] = useAccountsFormContext();
	const isPopup = useAppSelector((state) => state.app.appType === AppType.popup);
	const [isConnectLedgerModalOpen, setConnectLedgerModalOpen] = useState(forceShowLedger);
	return (
		<Overlay showModal title="Add Account" closeOverlay={() => navigate('/')}>
			<div className="w-full flex flex-col gap-8">
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
						onClick={async () => {
							ampli.openedConnectLedgerFlow({ sourceFlow });
							if (isPopup) {
								const { origin, pathname, hash } = window.location;
								await Browser.tabs.create({
									url: `${origin}${pathname}${hash}${
										hash.includes('showLedger') ? '' : `${hash.includes('?') ? '&' : '?'}showLedger`
									}`,
								});
								window.close();
							} else {
								setConnectLedgerModalOpen(true);
							}
						}}
					/>
				</div>
				<Section title="Create New">
					<Button
						variant="outline"
						size="tall"
						text="Create a new Passphrase Account"
						to="/accounts/protect-account?accountType=new-mnemonic"
						onClick={() => {
							setAccountFormValues({ type: 'new-mnemonic' });
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
