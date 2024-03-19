// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { useAppsBackend, useElementDimensions, useLocalStorage } from '@mysten/core';
import { Heading, LoadingIndicator, Text } from '@mysten/ui';
import { useQuery } from '@tanstack/react-query';
import clsx from 'clsx';
import { type ReactNode, useEffect, useRef } from 'react';

import Footer from '../footer/Footer';
import Header from '../header/Header';
import { useNetworkContext } from '~/context';
import { Banner } from '~/ui/Banner';
import { Network } from '~/utils/api/DefaultRpcClient';
import suiscanImg from '~/assets/explorer-suiscan.jpg';
import suivisionImg from '~/assets/explorer-suivision.jpg';
import suiscanImg2x from '~/assets/explorer-suiscan@2x.jpg';
import suivisionImg2x from '~/assets/explorer-suivision@2x.jpg';
import { ButtonOrLink } from '~/ui/utils/ButtonOrLink';
import { Image } from '~/ui/image/Image';
import { ArrowRight12, Sui, SuiLogoTxt } from '@mysten/icons';
import { useRedirectExplorerUrl } from '~/hooks/useRedirectExplorerUrl';
import { ampli } from '~/utils/analytics/ampli';

enum RedirectExplorer {
	SUISCAN = 'suiscan',
	SUIVISION = 'suivision',
}

export type PageLayoutProps = {
	gradient?: {
		content: ReactNode;
		size: 'lg' | 'md';
	};
	isError?: boolean;
	content: ReactNode;
	loading?: boolean;
};

const DEFAULT_HEADER_HEIGHT = 68;

function useRedirectExplorerOrder() {
	const [isSuiVisionFirst, setSuiVisionOrder] = useLocalStorage<boolean | undefined>(
		'is-suivision-first',
		undefined,
	);

	useEffect(() => {
		if (typeof isSuiVisionFirst === 'undefined') {
			setSuiVisionOrder(new Date().getMilliseconds() % 2 === 0);
		}
	}, [isSuiVisionFirst, setSuiVisionOrder]);

	return isSuiVisionFirst
		? [RedirectExplorer.SUIVISION, RedirectExplorer.SUISCAN]
		: [RedirectExplorer.SUISCAN, RedirectExplorer.SUIVISION];
}

function ImageLink({ type }: { type: RedirectExplorer }) {
	const { suiscanUrl, suivisionUrl } = useRedirectExplorerUrl();

	const href = type === RedirectExplorer.SUISCAN ? suiscanUrl : suivisionUrl;
	const src = type === RedirectExplorer.SUISCAN ? suiscanImg : suivisionImg;
	const srcSet =
		type === RedirectExplorer.SUISCAN
			? `${suiscanImg} 1x, ${suiscanImg2x} 2x`
			: `${suivisionImg} 1x, ${suivisionImg2x} 2x`;

	return (
		<div className="relative overflow-hidden rounded-3xl border border-gray-45 transition duration-300 ease-in-out hover:shadow-lg">
			<ButtonOrLink
				onClick={() => {
					ampli.redirectToExternalExplorer({
						name: type,
						url: href,
					});
				}}
				href={href}
				target="_blank"
				rel="noopener noreferrer"
			>
				<Image src={src} srcSet={srcSet} />
			</ButtonOrLink>
			<div className="absolute bottom-10 left-1/2 right-0 flex -translate-x-1/2 whitespace-nowrap">
				<ButtonOrLink
					className="flex w-full items-center justify-center gap-2 rounded-3xl bg-sui-dark px-3 py-2"
					onClick={() => {
						ampli.redirectToExternalExplorer({
							name: type,
							url: href,
						});
					}}
					href={href}
					target="_blank"
					rel="noopener noreferrer"
				>
					<Text variant="body/semibold" color="white">
						{type === RedirectExplorer.SUISCAN ? 'Visit Suiscan.xyz' : 'Visit Suivision.xyz'}
					</Text>
					<ArrowRight12 className="h-3 w-3 -rotate-45 text-white" />
				</ButtonOrLink>
			</div>
		</div>
	);
}

function RedirectContent() {
	const redirectExplorers = useRedirectExplorerOrder();

	return (
		<section className="flex flex-col justify-center gap-10 sm:flex-row">
			{redirectExplorers.map((type) => (
				<ImageLink key={type} type={type} />
			))}
		</section>
	);
}

function HeaderLink({ type }: { type: RedirectExplorer }) {
	const { suiscanUrl, suivisionUrl } = useRedirectExplorerUrl();
	const href = type === RedirectExplorer.SUISCAN ? suiscanUrl : suivisionUrl;
	const openWithLabel =
		type === RedirectExplorer.SUISCAN ? 'Open on Suiscan.xyz' : 'Open on Suivision.xyz';

	return (
		<ButtonOrLink
			href={href}
			target="_blank"
			className="flex items-center gap-2 border-b border-gray-100 py-1 text-heading5 font-semibold"
			onClick={() => {
				ampli.redirectToExternalExplorer({
					name: type,
					url: href,
				});
			}}
		>
			{openWithLabel} <ArrowRight12 className="h-4 w-4 -rotate-45" />
		</ButtonOrLink>
	);
}

export function RedirectHeader() {
	const { hasMatch } = useRedirectExplorerUrl();
	const redirectExplorers = useRedirectExplorerOrder();

	return (
		<section
			className="flex flex-col items-center justify-center gap-5 px-5 py-12 text-center sm:mb-20"
			style={{
				background: 'linear-gradient(159deg, #FAF8D2 50.65%, #F7DFD5 86.82%)',
			}}
		>
			<div className="flex items-center gap-1">
				<Sui className={clsx(hasMatch ? 'h-7.5 w-5' : 'h-11 w-9')} />
				<SuiLogoTxt className={clsx(hasMatch ? 'h-5 w-7.5' : 'h-7 w-11')} />
			</div>

			{hasMatch ? (
				<div className="flex flex-col gap-2">
					<Text variant="body/medium">
						The link that brought you here is no longer available on suiexplorer.com
					</Text>
					<div className="flex flex-col items-center justify-center gap-5 sm:flex-row">
						{redirectExplorers.map((type) => (
							<HeaderLink key={type} type={type} />
						))}
					</div>
				</div>
			) : (
				<Heading variant="heading3/semibold">Experience two amazing explorers on Sui!</Heading>
			)}
		</section>
	);
}

export function PageLayout({ gradient, content, loading, isError }: PageLayoutProps) {
	const enableExplorerRedirect = useFeatureIsOn('explorer-redirect');
	const [network] = useNetworkContext();
	const { request } = useAppsBackend();
	const outageOverride = useFeatureIsOn('network-outage-override');

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
	const renderNetworkDegradeBanner =
		outageOverride || (network === Network.MAINNET && data?.degraded);
	const headerRef = useRef<HTMLElement | null>(null);
	const [headerHeight] = useElementDimensions(headerRef, DEFAULT_HEADER_HEIGHT);

	const networkDegradeBannerCopy =
		network === Network.TESTNET
			? 'Sui Explorer (Testnet) is currently under-going maintenance. Some data may be incorrect or missing.'
			: "The explorer is running slower than usual. We're working to fix the issue and appreciate your patience.";

	return (
		<div className="relative min-h-screen w-full">
			<section ref={headerRef} className="fixed top-0 z-20 flex w-full flex-col">
				{renderNetworkDegradeBanner && (
					<Banner rounded="none" align="center" variant="warning" fullWidth>
						<div className="break-normal">{networkDegradeBannerCopy}</div>
					</Banner>
				)}
				{!enableExplorerRedirect && <Header />}
			</section>
			{enableExplorerRedirect && <RedirectHeader />}
			{loading && (
				<div className="absolute left-1/2 right-0 top-1/2 flex -translate-x-1/2 -translate-y-1/2 transform justify-center">
					<LoadingIndicator variant="lg" />
				</div>
			)}
			<main
				className="relative z-10 bg-offwhite"
				style={
					!isGradientVisible
						? {
								paddingTop: `${headerHeight}px`,
						  }
						: {}
				}
			>
				{isGradientVisible && !enableExplorerRedirect ? (
					<section
						style={{
							paddingTop: `${headerHeight}px`,
						}}
						className={clsx(
							'group/gradientContent',
							loading && 'bg-gradients-graph-cards',
							isError && 'bg-gradients-failure',
							!isError && 'bg-gradients-graph-cards',
						)}
					>
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
				{!loading && (
					<section className="mx-auto max-w-[1440px] p-5 pb-20 sm:py-8 md:p-10 md:pb-20">
						{enableExplorerRedirect ? <RedirectContent /> : content}
					</section>
				)}
			</main>
			<Footer />
		</div>
	);
}
