// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cn from 'classnames';
import { createContext, type ReactNode, useState } from 'react';

import { WalletSettingsButton } from '../../components/menu/button/WalletSettingsButton';
import { useAppSelector } from '../../hooks';
import { AppType } from '../../redux/slices/app/AppType';
import DappStatus from '../dapp-status';
import { Header } from '../header/Header';
import { Toaster } from '../toaster';
import { ErrorBoundary } from '_components/error-boundary';
import { MenuContent } from '_components/menu';
import { Navigation } from '_components/navigation';

export const PageMainLayoutContext = createContext<HTMLDivElement | null>(null);

export type PageMainLayoutProps = {
	children: ReactNode | ReactNode[];
	bottomNavEnabled?: boolean;
	topNavMenuEnabled?: boolean;
	dappStatusEnabled?: boolean;
};

export function PageMainLayout({
	children,
	bottomNavEnabled = false,
	topNavMenuEnabled = false,
	dappStatusEnabled = false,
}: PageMainLayoutProps) {
	const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	const appType = useAppSelector((state) => state.app.appType);
	const isFullScreen = appType === AppType.fullscreen;
	const [titlePortalContainer, setTitlePortalContainer] = useState<HTMLDivElement | null>(null);

	return (
		<div
			className={cn(
				'flex flex-col flex-nowrap items-stretch justify-center flex-1 w-full max-h-full bg-gradients-graph-cards overflow-hidden',
				isFullScreen ? 'rounded-xl' : '',
			)}
		>
			<Header
				networkName={networkName}
				middleContent={dappStatusEnabled ? <DappStatus /> : <div ref={setTitlePortalContainer} />}
				rightContent={topNavMenuEnabled ? <WalletSettingsButton /> : undefined}
			/>
			<div className="relative flex flex-col flex-nowrap flex-grow overflow-hidden rounded-t-xl">
				<div className="flex flex-col flex-nowrap bg-white flex-grow overflow-y-auto overflow-x-hidden rounded-t-xl">
					<main className={cn('flex-grow w-full', { 'p-5': bottomNavEnabled })}>
						<PageMainLayoutContext.Provider value={titlePortalContainer}>
							<ErrorBoundary>{children}</ErrorBoundary>
						</PageMainLayoutContext.Provider>
					</main>
					{bottomNavEnabled ? <Navigation /> : null}
					<Toaster bottomNavEnabled={bottomNavEnabled} />
				</div>
				{topNavMenuEnabled ? <MenuContent /> : null}
			</div>
		</div>
	);
}
