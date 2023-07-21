// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SocialButton } from '../../shared/SocialButton';
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';
import { ampli } from '_src/shared/analytics/ampli';

function WelcomePage() {
	const checkingInitialized = useInitializedGuard(false);
	return (
		<PageLayout forceFullscreen>
			<Loading loading={checkingInitialized}>
				<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center w-popup-width h-popup-height p-10">
					<div className="shrink-0">
						<Logo />
					</div>
					<div className="text-center mx-auto mt-4">
						<Heading variant="heading2" color="gray-90" as="h1" weight="bold">
							Welcome to Sui Wallet
						</Heading>
						<div className="mt-2">
							<Text variant="pBody" color="steel-dark" weight="medium">
								Connecting you to the decentralized web and Sui network.
							</Text>
						</div>
					</div>
					<div className="w-full h-full bg-gray-50 mt-10">
						TODO: Replace me with a splash image!
					</div>
					<div className="flex flex-col gap-4 mt-7.5 w-full items-center">
						<Text variant="pBody" color="steel-dark" weight="medium">
							Sign in with your preferred service
						</Text>
						<div className="flex gap-2 w-full">
							<div className="flex-1">
								<SocialButton
									provider="google"
									onClick={() => {
										// eslint-disable-next-line no-console
										console.log('TODO: Open OAuth flow');
										ampli.clickedSocialSignInButton({
											signInProvider: 'Google',
											sourceFlow: 'Onboarding',
										});
									}}
								/>
							</div>
							<div className="flex-1">
								<SocialButton
									provider="twitch"
									onClick={() => {
										// eslint-disable-next-line no-console
										console.log('TODO: Open OAuth flow');
										ampli.clickedSocialSignInButton({
											signInProvider: 'Twitch',
											sourceFlow: 'Onboarding',
										});
									}}
								/>
							</div>
							<div className="flex-1">
								<SocialButton
									provider="facebook"
									onClick={() => {
										// eslint-disable-next-line no-console
										console.log('TODO: Open OAuth flow');
										ampli.clickedSocialSignInButton({
											signInProvider: 'Facebook',
											sourceFlow: 'Onboarding',
										});
									}}
								/>
							</div>
							<div className="flex-1">
								<SocialButton
									provider="microsoft"
									onClick={() => {
										// eslint-disable-next-line no-console
										console.log('TODO: Open OAuth flow');
										ampli.clickedSocialSignInButton({
											signInProvider: 'Microsoft',
											sourceFlow: 'Onboarding',
										});
									}}
								/>
							</div>
						</div>
						<Button
							to="/onboarding/add-account"
							size="tall"
							variant="secondary"
							text="More Options"
						/>
					</div>
				</div>
			</Loading>
		</PageLayout>
	);
}

export default WelcomePage;
