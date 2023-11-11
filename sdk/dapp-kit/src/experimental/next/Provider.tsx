// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
'use client';

import { CookiesProvider, useCookies } from 'next-client-cookies';
import type { cookies as nextCookies } from 'next/headers';
import { useRouter } from 'next/navigation';

import type { WalletProviderProps } from '../../components/WalletProvider.js';
import { WalletProvider } from '../../components/WalletProvider.js';
import { createCookieStore } from './store.js';

export type NextWalletProviderProps = Omit<WalletProviderProps, 'storage'> & {
	cookies: ReturnType<ReturnType<typeof nextCookies>['getAll']>;
};

export function NextWalletProviderWithCookies(props: Omit<NextWalletProviderProps, 'cookies'>) {
	const router = useRouter();
	const cookies = useCookies();

	return (
		<WalletProvider
			{...props}
			// The Next.js provider automatically connects to the wallet by default.
			autoConnect={'autoConnect' in props ? props.autoConnect : true}
			storage={createCookieStore(cookies, () => {
				// Whenever the cookies change, clear the router cache:
				router.refresh();
			})}
		/>
	);
}

export function NextWalletProvider({ cookies, ...props }: NextWalletProviderProps) {
	return (
		<CookiesProvider value={cookies}>
			<NextWalletProviderWithCookies {...props} />
		</CookiesProvider>
	);
}
