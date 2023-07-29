// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { useOnrampProviders } from './useOnrampProviders';
import { ampli } from '_src/shared/analytics/ampli';
import Alert from '_src/ui/app/components/alert';
import Overlay from '_src/ui/app/components/overlay';
import { useActiveAddress } from '_src/ui/app/hooks';
import { Heading } from '_src/ui/app/shared/heading';

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

	if (!providers || !providers.length) return null;

	return (
		<Overlay
			showModal
			title="Buy"
			closeOverlay={() => {
				navigate('/tokens');
			}}
		>
			<div className="w-full">
				<button
					onClick={() => {
						mutate();
					}}
					className="w-full p-6 bg-sui/10 rounded-2xl flex items-center gap-2.5 border-0 cursor-pointer"
				>
					<preferredProvider.icon />
					<Heading variant="heading6" weight="semibold" color="hero-dark">
						Continue with {preferredProvider.name}
					</Heading>
				</button>

				{!!error && (
					<div className="mt-2">
						<Alert>An unexpected error occurred. Please try again later.</Alert>
					</div>
				)}

				<div className="flex flex-col gap-4 mt-5">
					{otherProviders.map((provider) => (
						<button
							key={provider.key}
							className="block font-medium text-body text-center text-steel bg-transparent border-none cursor-pointer"
							onClick={() => setPreferredProvider(provider.key)}
						>
							Already have a {provider.name} account?
						</button>
					))}
				</div>
			</div>
		</Overlay>
	);
}
