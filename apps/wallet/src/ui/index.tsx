// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@fontsource-variable/inter';
import '@fontsource-variable/red-hat-mono';

import { ErrorBoundary } from '_components/error-boundary';
import { initAppType } from '_redux/slices/app';
import { AppType, getFromLocationSearch } from '_redux/slices/app/AppType';
import { initAmplitude } from '_src/shared/analytics/amplitude';
import { setAttributes } from '_src/shared/experimentation/features';
import initSentry from '_src/ui/app/helpers/sentry';
import store from '_store';
import { thunkExtras } from '_store/thunk-extras';
import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { KioskClientProvider } from '@mysten/core/src/components/KioskClientProvider';
import { SuiClientProvider } from '@mysten/dapp-kit';
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client';
import cn from 'clsx';
import { Fragment, StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Provider } from 'react-redux';
import { HashRouter } from 'react-router-dom';

import App from './app';
import { walletApiProvider } from './app/ApiProvider';
import { AccountsFormProvider } from './app/components/accounts/AccountsFormContext';
import { UnlockAccountProvider } from './app/components/accounts/UnlockAccountContext';
import { ZkLoginAccountWarningModal } from './app/components/accounts/ZkLoginAccountWaringModal';
import { SuiLedgerClientProvider } from './app/components/ledger/SuiLedgerClientProvider';
import { growthbook } from './app/experimentation/feature-gating';
import { persister, queryClient } from './app/helpers/queryClient';
import { useAppSelector } from './app/hooks';

import './styles/global.scss';
import 'bootstrap-icons/font/bootstrap-icons.scss';

async function init() {
	if (process.env.NODE_ENV === 'development') {
		Object.defineProperty(window, 'store', { value: store });
	}
	store.dispatch(initAppType(getFromLocationSearch(window.location.search)));
	await thunkExtras.background.init(store.dispatch);
	const { apiEnv, customRPC } = store.getState().app;
	setAttributes({ apiEnv, customRPC });
}

function renderApp() {
	const rootDom = document.getElementById('root');
	if (!rootDom) {
		throw new Error('Root element not found');
	}
	const root = createRoot(rootDom);
	root.render(
		<StrictMode>
			<Provider store={store}>
				<AppWrapper />
			</Provider>
		</StrictMode>,
	);
}

function AppWrapper() {
	const network = useAppSelector(({ app: { apiEnv, customRPC } }) => `${apiEnv}_${customRPC}`);
	const isFullscreen = useAppSelector((state) => state.app.appType === AppType.fullscreen);
	return (
		<GrowthBookProvider growthbook={growthbook}>
			<HashRouter>
				<SuiLedgerClientProvider>
					{/*
					 * NOTE: We set a key here to force the entire react tree to be re-created when the network changes so that
					 * the RPC client instance (api.instance.fullNode) is updated correctly. In the future, we should look into
					 * making the API provider instance a reactive value and moving it out of the redux-thunk middleware
					 */}
					<Fragment key={network}>
						<PersistQueryClientProvider
							client={queryClient}
							persistOptions={{
								persister,
								dehydrateOptions: {
									shouldDehydrateQuery: ({ meta }) => !meta?.skipPersistedCache,
								},
							}}
						>
							<SuiClientProvider
								networks={{ [walletApiProvider.apiEnv]: walletApiProvider.instance.fullNode }}
							>
								<KioskClientProvider>
									<AccountsFormProvider>
										<UnlockAccountProvider>
											<div
												className={cn(
													'relative flex flex-col flex-nowrap items-center justify-center w-popup-width min-h-popup-minimum max-h-popup-height h-screen overflow-hidden',
													isFullscreen && 'shadow-lg rounded-xl',
												)}
											>
												<ErrorBoundary>
													<App />
													<ZkLoginAccountWarningModal />
												</ErrorBoundary>
												<div id="overlay-portal-container"></div>
												<div id="toaster-portal-container"></div>
											</div>
										</UnlockAccountProvider>
									</AccountsFormProvider>
								</KioskClientProvider>
							</SuiClientProvider>
						</PersistQueryClientProvider>
					</Fragment>
				</SuiLedgerClientProvider>
			</HashRouter>
		</GrowthBookProvider>
	);
}

(async () => {
	await init();
	initSentry();
	initAmplitude();
	renderApp();
})();
