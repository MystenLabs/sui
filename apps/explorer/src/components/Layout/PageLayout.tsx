// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadingIndicator } from '@mysten/ui';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import Footer from '../footer/Footer';
import Header from '../header/Header';

export type PageLayoutProps = {
	gradient?: {
		content: ReactNode;
		size: 'lg' | 'md';
	};
	isError?: boolean;
	content: ReactNode;
	loading?: boolean;
	backgroundGradient?: boolean;
};

export function PageLayout({
	gradient,
	content,
	loading,
	isError,
	backgroundGradient,
}: PageLayoutProps) {
	const isGradientVisible = !!gradient;

	return (
		<div
			className={clsx(
				'relative min-h-screen w-full',
				isGradientVisible && backgroundGradient && isError && 'bg-gradients-failure-start',
				isGradientVisible && backgroundGradient && !isError && 'bg-gradients-graph-cards-start',
			)}
		>
			<Header />
			{loading && (
				<div className="absolute left-1/2 right-0 top-1/2 flex -translate-x-1/2 -translate-y-1/2 transform justify-center">
					<LoadingIndicator variant="lg" />
				</div>
			)}
			<main className="relative z-10 bg-offwhite">
				{isGradientVisible ? (
					<section
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
					<section className="mx-auto max-w-[1440px] p-5 sm:py-8 md:p-10">{content}</section>
				)}
			</main>
			<Footer />
		</div>
	);
}
