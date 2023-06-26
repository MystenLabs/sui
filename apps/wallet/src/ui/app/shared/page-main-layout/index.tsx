// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { createContext, type ReactNode, useState } from 'react';

import { useAppSelector } from '../../hooks';
import { AppType } from '../../redux/slices/app/AppType';
import DappStatus from '../dapp-status';
import { Header } from '../header/Header';
import { Toaster } from '../toaster';
import { ErrorBoundary } from '_components/error-boundary';
import { MenuButton, MenuContent } from '_components/menu';
import Navigation from '_components/navigation';

import st from './PageMainLayout.module.scss';

export const PageMainLayoutContext = createContext<HTMLDivElement | null>(null);

export type PageMainLayoutProps = {
	children: ReactNode | ReactNode[];
	bottomNavEnabled?: boolean;
	topNavMenuEnabled?: boolean;
	dappStatusEnabled?: boolean;
	className?: string;
};

export default function PageMainLayout({
	children,
	bottomNavEnabled = false,
	topNavMenuEnabled = false,
	dappStatusEnabled = false,
	className,
}: PageMainLayoutProps) {
	const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	const appType = useAppSelector((state) => state.app.appType);
	const isFullScreen = appType === AppType.fullscreen;
	const [titlePortalContainer, setTitlePortalContainer] = useState<HTMLDivElement | null>(null);
	return (
		<div
			className={cl(st.container, {
				[st.fullScreenContainer]: isFullScreen,
			})}
		>
			<Header
				networkName={networkName}
				middleContent={dappStatusEnabled ? <DappStatus /> : <div ref={setTitlePortalContainer} />}
				rightContent={topNavMenuEnabled ? <MenuButton /> : undefined}
			/>
			<div
				className={cl(st.content, {
					[st.fullScreenContent]: isFullScreen,
				})}
			>
				<main className={cl(st.main, { [st.withNav]: bottomNavEnabled }, className)}>
					<PageMainLayoutContext.Provider value={titlePortalContainer}>
						<ErrorBoundary>{children}</ErrorBoundary>
					</PageMainLayoutContext.Provider>
				</main>
				{bottomNavEnabled ? <Navigation /> : null}
				{topNavMenuEnabled ? <MenuContent /> : null}
				<Toaster bottomNavEnabled={bottomNavEnabled} />
			</div>
		</div>
	);
}
