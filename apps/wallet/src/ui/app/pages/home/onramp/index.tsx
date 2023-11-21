// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ampli } from '_src/shared/analytics/ampli';
import Alert from '_src/ui/app/components/alert';
import Overlay from '_src/ui/app/components/overlay';
import { SectionHeader } from '_src/ui/app/components/SectionHeader';
import { useActiveAddress } from '_src/ui/app/hooks';
import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import { Heading } from '_src/ui/app/shared/heading';
import { Text } from '_src/ui/app/shared/text';
import { useMutation } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { useOnrampProviders } from './useOnrampProviders';

export function Onramp() {
	const navigate = useNavigate();
	const address = useActiveAddress();
	const { providers, setPreferredProvider } = useOnrampProviders();

	const [preferredProvider, ...otherProviders] = providers ?? [];

	const { mutate, error } = useMutation({
		mutationKey: ['onramp', 'get-provider-url'],
		mutationFn: () => {
			return preferredProvider.getUrl(address!);
		},
		onSuccess: (data) => {
			ampli.visitedFiatOnRamp({ providerName: preferredProvider.name });
			window.open(data, '_blank');
		},
	});

	useEffect(() => {
		// This shouldn't happen, but if you land on this page directly and no on-ramps are supported, just bail out
		if (providers && providers.length === 0) {
			navigate('/tokens');
		}
	}, [providers, navigate]);

	const isGuardLoading = useUnlockedGuard();

	if (!providers || !providers.length || isGuardLoading) return null;

	return (
		<Overlay
			showModal
			title="Buy"
			closeOverlay={() => {
				navigate('/tokens');
			}}
		>
			<div className="w-full flex flex-col gap-5">
				{preferredProvider && (
					<Text variant="body" weight="medium" color="steel-darker">
						Continue checkout out with one of our partners:{' '}
					</Text>
				)}
				<button
					onClick={() => {
						mutate();
					}}
					className="w-full p-4 bg-sui/10 rounded-2xl flex flex-col gap-2.5 cursor-pointer border boder-solid border-hero/20 hover:border-hero/40"
				>
					<span className="flex w-full">
						<preferredProvider.icon className="mr-auto w-10 h-10" />
					</span>

					<Heading variant="heading6" weight="semibold" color="hero-dark">
						Continue with {preferredProvider.name}
					</Heading>
				</button>

				{!!error && (
					<div className="mt-2">
						<Alert>An unexpected error occurred. Please try again later.</Alert>
					</div>
				)}
				<SectionHeader title="Or" />
				<div className="flex gap-2">
					{otherProviders.map((provider) => {
						return (
							<button
								key={provider.key}
								className="flex gap-3 flex-1 items-center bg-transparent border border-solid border-gray-45 cursor-pointer rounded-4lg p-3.5 hover:border-gray-60"
								onClick={() => setPreferredProvider(provider.key)}
							>
								<provider.icon className="w-8 h-8" />
								<Text variant="body" weight="semibold" color="gray-90">
									{provider.name}
								</Text>
							</button>
						);
					})}
				</div>
			</div>
		</Overlay>
	);
}
