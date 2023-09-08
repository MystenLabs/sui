// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useState } from 'react';
import { useCountAccountsByType } from '../../hooks/useCountAccountByType';
import { SocialButton } from '../../shared/SocialButton';
import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { ampli, type ClickedSocialSignInButtonProperties } from '_src/shared/analytics/ampli';

const zkLoginProviders: {
	provider: ZkProvider;
	disabled?: boolean;
	hidden?: boolean;
	tooltip?: string;
}[] = [
	{ provider: 'google' },
	{ provider: 'twitch', disabled: true, tooltip: 'Coming soon' },
	{ provider: 'facebook', disabled: true, tooltip: 'Coming soon' },
	{ provider: 'microsoft', disabled: true, hidden: true },
];

const providerToAmpli: Record<ZkProvider, ClickedSocialSignInButtonProperties['signInProvider']> = {
	google: 'Google',
	twitch: 'Twitch',
	facebook: 'Facebook',
	microsoft: 'Microsoft',
};

export type ZkLoginButtonsProps = {
	layout: 'column' | 'row';
	showLabel?: boolean;
	buttonsDisabled?: boolean;
	onButtonClick?: (provider: ZkProvider) => Promise<void> | void;
	sourceFlow: string;
	forcedZkLoginProvider?: ZkProvider | null;
};

export function ZkLoginButtons({
	layout,
	showLabel = false,
	onButtonClick,
	buttonsDisabled,
	sourceFlow,
	forcedZkLoginProvider,
}: ZkLoginButtonsProps) {
	const [createInProgressProvider, setCreateInProgressProvider] = useState<ZkProvider | null>(null);
	const { data: accountsTotalByType, isLoading } = useCountAccountsByType();
	return (
		<div
			className={cx('flex w-full', {
				'flex-col gap-3': layout === 'column',
				'flex-row gap-2': layout === 'row',
			})}
		>
			{zkLoginProviders
				.filter(({ hidden }) => !hidden)
				.map(({ provider, disabled, tooltip }) => (
					<div key={provider} className="flex-1">
						<SocialButton
							title={accountsTotalByType?.zk?.extra?.[provider] ? 'Already signed-in' : tooltip}
							provider={provider}
							onClick={async () => {
								ampli.clickedSocialSignInButton({
									signInProvider: providerToAmpli[provider],
									sourceFlow,
								});
								setCreateInProgressProvider(provider);
								try {
									if (onButtonClick) {
										await onButtonClick(provider);
									}
								} finally {
									setCreateInProgressProvider(null);
								}
							}}
							disabled={
								disabled ||
								buttonsDisabled ||
								createInProgressProvider !== null ||
								isLoading ||
								!!accountsTotalByType?.zk?.extra?.[provider] ||
								!!forcedZkLoginProvider
							}
							loading={createInProgressProvider === provider || forcedZkLoginProvider === provider}
							showLabel={showLabel}
						/>
					</div>
				))}
		</div>
	);
}
