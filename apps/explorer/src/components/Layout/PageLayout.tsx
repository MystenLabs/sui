// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { LoadingIndicator } from '@mysten/ui';
import { useQuery } from '@tanstack/react-query';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import Footer from '../footer/Footer';
import Header from '../header/Header';
import { useNetworkContext } from '~/context';
import { Banner } from '~/ui/Banner';
import { Network } from '~/utils/api/DefaultRpcClient';

export type PageLayoutProps = {
	gradient?: {
		content: ReactNode;
		size: 'lg' | 'md';
		type?: 'success' | 'error';
	};
	content: ReactNode;
	error?: string;
	loading?: boolean;
};

export function PageLayout({ gradient, content, loading }: PageLayoutProps) {
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
	const isGradientVisible = !!gradient;
	const isError = gradient?.type === 'error';

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
				{isGradientVisible ? (
					<section
						className={clsx(
							'group/gradientContent',
							isGradientVisible && isError && 'bg-gradients-failure',
							isGradientVisible && !isError && 'bg-gradients-graph-cards',
						)}
					>
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
						<div
							className={clsx(
								'mx-auto max-w-[1440px] py-8 lg:px-6 xl:px-10',
								gradient.size === 'lg' && 'px-4 xl:py-12',
								gradient.size === 'md' && 'px-4',
							)}
						>
							{gradient.content}
						</div>
					</section>
				) : null}
				{loading ? (
					<div className="absolute left-1/2 right-0 top-1/2 flex -translate-x-1/2 -translate-y-1/2 transform justify-center">
						<LoadingIndicator variant="lg" />
					</div>
				) : (
					<section className="mx-auto max-w-[1440px] p-5 sm:py-8 md:p-10">{content}</section>
				)}
			</main>
			<Footer />
		</div>
	);
}

//mx-auto max-w-[1440px] px-5 py-8 md:p-10
