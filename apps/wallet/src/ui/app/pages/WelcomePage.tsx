// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { toast } from 'react-hot-toast';
import { useCreateAccountsMutation } from '../hooks/useCreateAccountMutation';
import { SocialButton } from '../shared/SocialButton';
import { Toaster } from '../shared/toaster';
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';
import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { ampli } from '_src/shared/analytics/ampli';
import WelcomeSplash from '_src/ui/assets/images/WelcomeSplash.svg';

export function WelcomePage() {
	const isInitializedLoading = useInitializedGuard(false);
	const createAccountsMutation = useCreateAccountsMutation();
	const [createInProgressProvider, setCreateInProgressProvider] = useState<ZkProvider | null>(null);
	return (
		<PageLayout forceFullscreen>
			<Loading loading={isInitializedLoading}>
				<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-7 py-6 h-full overflow-auto">
					<div className="shrink-0">
						<Logo />
					</div>
					<div className="text-center mx-auto mt-2">
						<Heading variant="heading2" color="gray-90" as="h1" weight="bold">
							Welcome to Sui Wallet
						</Heading>
						<div className="mt-2">
							<Text variant="pBody" color="steel-dark" weight="medium">
								Connecting you to the decentralized web and Sui network.
							</Text>
						</div>
					</div>
					<div className="w-full h-full mt-3.5 flex justify-center">
						<WelcomeSplash role="img" />
					</div>
					<div className="flex flex-col gap-3 mt-3.5 w-full items-center">
						<Text variant="pBody" color="steel-dark" weight="medium">
							Sign in with your preferred service
						</Text>
						<div className="flex gap-2 w-full">
							<div className="flex-1">
								<SocialButton
									provider="google"
									onClick={() => {
										setCreateInProgressProvider('Google');
										ampli.clickedSocialSignInButton({
											signInProvider: 'Google',
											sourceFlow: 'Onboarding',
										});
										createAccountsMutation.mutate(
											{
												type: 'zk',
												provider: 'Google',
											},
											{
												onError: (error) => {
													toast.error(
														(error as Error)?.message ||
															'Failed to create account. (Unknown error)',
													);
												},
												onSettled: () => {
													setCreateInProgressProvider(null);
												},
											},
										);
									}}
									disabled={createAccountsMutation.isLoading}
									loading={createInProgressProvider === 'Google'}
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
									disabled
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
									disabled
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
									disabled
								/>
							</div>
						</div>
						<Button
							to="/accounts/add-account?sourceFlow=Onboarding"
							size="tall"
							variant="secondary"
							text="More Options"
							disabled
						/>
					</div>
				</div>
			</Loading>
			<Toaster />
		</PageLayout>
	);
}
