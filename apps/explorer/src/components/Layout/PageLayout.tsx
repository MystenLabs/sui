// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import Footer from '../footer/Footer';
import Header from '../header/Header';
import { useNetworkContext } from '~/context';
import { Banner } from '~/ui/Banner';
import { Network } from '~/utils/api/DefaultRpcClient';

export type PageLayoutProps = {
	gradientContent?: {
		content: ReactNode;
		size: 'lg' | 'md';
	};
	content: ReactNode;
	error?: string;
};

export function PageLayout({ gradientContent, content, error }: PageLayoutProps) {
	const [network] = useNetworkContext();
	const { request } = useAppsBackend();
	const { data } = useQuery({
		queryKey: ['apps-backend', 'monitor-network'],
		queryFn: () =>
			request<{ degraded: boolean }>('monitor-network', {
				project: 'EXPLORER',
			}),
		// Keep cached for 2 minutes:
		staleTime: 2 * 60 * 1000,
		retry: false,
		enabled: network === Network.MAINNET,
	});
	const isGradientVisible = !!gradientContent;
	const isError = !!error;
	console.log('isError', isGradientVisible, isError);

	return (
		<div
			className={clsx(
				'w-full',
				isGradientVisible && isError && 'bg-gradients-failure-start',
				isGradientVisible && !isError && 'bg-gradients-graph-cards-start',
			)}
		>
			<Header />
			<main className="relative z-10 min-h-screen bg-offwhite">
				{network === Network.MAINNET && data?.degraded && (
					<div className={clsx(isGradientVisible && 'bg-gradients-graph-cards-bg')}>
						<div className="mx-auto max-w-[1440px] px-4 pt-3 lg:px-6 xl:px-10">
							<Banner variant="warning" border fullWidth>
								We&rsquo;re sorry that the explorer is running slower than usual. We&rsquo;re
								working to fix the issue and appreciate your patience.
							</Banner>
						</div>
					</div>
				)}
				{isGradientVisible ? (
					<section
						className={clsx(
							'group/gradientContent',
							isGradientVisible && isError && 'bg-gradients-failure',
							isGradientVisible && !isError && 'bg-gradients-graph-cards',
						)}
					>
						<div
							className={clsx(
								'mx-auto max-w-[1440px] py-8 lg:px-6 xl:px-10',
								gradientContent.size === 'lg' && 'px-4 xl:py-12',
								gradientContent.size === 'md' && 'px-4',
							)}
						>
							{gradientContent.content}
						</div>
					</section>
				) : null}
				<section className="mx-auto max-w-[1440px] px-4 py-6 pb-16 lg:px-6 xl:p-10 xl:pb-16">
					{content}
				</section>
			</main>
			<Footer />
		</div>
	);
}
