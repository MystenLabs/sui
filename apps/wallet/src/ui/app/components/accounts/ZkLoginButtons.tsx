// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	zkLoginProviderDataMap,
	type ZkLoginProvider,
} from '_src/background/accounts/zklogin/providers';
import { ampli, type ClickedSocialSignInButtonProperties } from '_src/shared/analytics/ampli';
import { cx } from 'class-variance-authority';
import { useState } from 'react';

import { useCountAccountsByType } from '../../hooks/useCountAccountByType';
import { SocialButton } from '../../shared/SocialButton';

const zkLoginProviders = Object.entries(zkLoginProviderDataMap)
	.filter(([_, { hidden }]) => !hidden)
	.map(([provider, { enabled, order }]) => ({
		provider: provider as ZkLoginProvider,
		enabled,
		tooltip: !enabled ? 'Coming soon!' : undefined,
		order,
	}))
	.sort((a, b) => a.order - b.order);

const providerToAmpli: Record<
	ZkLoginProvider,
	ClickedSocialSignInButtonProperties['signInProvider']
> = {
	google: 'Google',
	twitch: 'Twitch',
	facebook: 'Facebook',
	kakao: 'Kakao',
};

export type ZkLoginButtonsProps = {
	layout: 'column' | 'row';
	showLabel?: boolean;
	buttonsDisabled?: boolean;
	onButtonClick?: (provider: ZkLoginProvider) => Promise<void> | void;
	sourceFlow: string;
	forcedZkLoginProvider?: ZkLoginProvider | null;
};

export function ZkLoginButtons({
	layout,
	showLabel = false,
	onButtonClick,
	buttonsDisabled,
	sourceFlow,
	forcedZkLoginProvider,
}: ZkLoginButtonsProps) {
	const [createInProgressProvider, setCreateInProgressProvider] = useState<ZkLoginProvider | null>(
		null,
	);
	const { data: accountsTotalByType, isPending } = useCountAccountsByType();
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
						title={accountsTotalByType?.zkLogin?.extra?.[provider] ? 'Already signed-in' : tooltip}
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
							isPending ||
							!!accountsTotalByType?.zkLogin?.extra?.[provider] ||
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
