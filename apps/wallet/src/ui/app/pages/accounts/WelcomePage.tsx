// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { toast } from 'react-hot-toast';
import { useAccountsFormContext } from '../../components/accounts/AccountsFormContext';
import { useCreateAccountsMutation } from '../../hooks/useCreateAccountMutation';
import { SocialButton } from '../../shared/SocialButton';
import { Button } from '_app/shared/ButtonUI';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useFullscreenGuard, useInitializedGuard } from '_hooks';
import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { type ClickedSocialSignInButtonProperties, ampli } from '_src/shared/analytics/ampli';
import WelcomeSplash from '_src/ui/assets/images/WelcomeSplash.svg';

const zkLoginProviders: {
	provider: ZkProvider;
	ampliSignInProvider: ClickedSocialSignInButtonProperties['signInProvider'];
	disabled?: boolean;
}[] = [
	{ provider: 'google', ampliSignInProvider: 'Google' },
	{ provider: 'twitch', ampliSignInProvider: 'Twitch', disabled: true },
	{ provider: 'facebook', ampliSignInProvider: 'Facebook', disabled: true },
	{ provider: 'microsoft', ampliSignInProvider: 'Microsoft', disabled: true },
];

export function WelcomePage() {
	const isFullscreenGuardLoading = useFullscreenGuard(true);
	const isInitializedLoading = useInitializedGuard(false);
	const [, setAccountsFormValues] = useAccountsFormContext();
	const createAccountsMutation = useCreateAccountsMutation();
	const [createInProgressProvider, setCreateInProgressProvider] = useState<ZkProvider | null>(null);
	const buttonsDisabled = createAccountsMutation.isLoading || createAccountsMutation.isSuccess;
	return (
		<Loading loading={isInitializedLoading || isFullscreenGuardLoading}>
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
						{zkLoginProviders.map(({ provider, ampliSignInProvider, disabled }) => (
							<div key={provider} className="flex-1">
								<SocialButton
									provider={provider}
									onClick={() => {
										setCreateInProgressProvider(provider);
										ampli.clickedSocialSignInButton({
											signInProvider: ampliSignInProvider,
											sourceFlow: 'Onboarding',
										});
										setAccountsFormValues({ type: 'zk', provider });
										createAccountsMutation.mutate(
											{
												type: 'zk',
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
									disabled={disabled || buttonsDisabled}
									loading={createInProgressProvider === provider}
								/>
							</div>
						))}
					</div>
					<Button
						to="/accounts/add-account?sourceFlow=Onboarding"
						size="tall"
						variant="secondary"
						text="More Options"
						disabled={buttonsDisabled}
					/>
				</div>
			</div>
		</Loading>
	);
}
