// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LedgerLogo17 as LedgerLogo } from '@mysten/icons';
import { type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { SocialButton } from '../shared/SocialButton';
import { Button } from '_app/shared/ButtonUI';
import { Text } from '_app/shared/text';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';
import { ampli } from '_src/shared/analytics/ampli';

type AddAccountOverlayProps = {
	showSocialSignInButtons?: boolean;
};

function AddAccountOverlay({ showSocialSignInButtons = false }: AddAccountOverlayProps) {
	const checkingInitialized = useInitializedGuard(false);
	const navigate = useNavigate();

	return (
		<PageLayout forceFullscreen>
			<Loading loading={checkingInitialized}>
				<Overlay showModal title="Add Accounts" closeOverlay={() => navigate('/')}>
					<div className="w-full flex flex-col gap-8 pt-3">
						<div className="flex flex-col gap-3">
							{showSocialSignInButtons && (
								<>
									<SocialButton
										provider="google"
										showLabel
										onClick={() => {
											// eslint-disable-next-line no-console
											console.log('TODO: Open OAuth flow');
											ampli.clickedSocialSignInButton({
												signInProvider: 'Google',
												sourceFlow: 'Add account menu',
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
												sourceFlow: 'Add account menu',
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
												sourceFlow: 'Add account menu',
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
												sourceFlow: 'Add account menu',
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
								to="/accounts/connect-ledger-modal"
								onClick={async () => {
									ampli.openedConnectLedgerFlow({ sourceFlow: 'Onboarding' });
								}}
							/>
						</div>
						<Section title="Create New">
							<Button
								variant="outline"
								size="tall"
								text="Create a new Passphrase Account"
								to="/accounts/create-passphrase-account"
								onClick={async () => {
									// ampli.openedConnectLedgerFlow({ sourceFlow: 'Onboarding' });
								}}
							/>
						</Section>
						<Section title="Import Existing Accounts">
							<Button
								variant="outline"
								size="tall"
								text="Import Passphrase"
								to="/accounts/import-passphrase"
								onClick={async () => {
									// ampli.openedConnectLedgerFlow({ sourceFlow: 'Onboarding' });
								}}
							/>
							<Button
								variant="outline"
								size="tall"
								text="Import Private Key"
								to="/accounts/import-private-key"
								onClick={async () => {
									// ampli.openedConnectLedgerFlow({ sourceFlow: 'Onboarding' });
								}}
							/>
						</Section>
					</div>
				</Overlay>
			</Loading>
		</PageLayout>
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

export default AddAccountOverlay;
