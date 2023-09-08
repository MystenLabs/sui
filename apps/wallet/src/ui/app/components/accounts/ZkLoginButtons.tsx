// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useState } from 'react';
import { useCountAccountsByType } from '../../hooks/useCountAccountByType';
import { SocialButton } from '../../shared/SocialButton';
import { zkProviderDataMap, type ZkProvider } from '_src/background/accounts/zk/providers';
import { ampli, type ClickedSocialSignInButtonProperties } from '_src/shared/analytics/ampli';

const zkLoginProviders = Object.entries(zkProviderDataMap).map(([provider, { enabled }]) => ({
	provider: provider as ZkProvider,
	enabled,
	tooltip: !enabled ? 'Coming soon!' : undefined,
}));

const providerToAmpli: Record<ZkProvider, ClickedSocialSignInButtonProperties['signInProvider']> = {
	google: 'Google',
	twitch: 'Twitch',
	facebook: 'Facebook',
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
			{zkLoginProviders.map(({ provider, enabled, tooltip }) => (
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
							!enabled ||
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
