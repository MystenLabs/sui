// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './global.css';

import { Banner } from 'fumadocs-ui/components/banner';
import { RootProvider } from 'fumadocs-ui/provider';
import { Inter } from 'next/font/google';
import type { ReactNode } from 'react';

const inter = Inter({
	subsets: ['latin'],
});

export default function Layout({ children }: { children: ReactNode }) {
	return (
		<html lang="en" className={inter.className} suppressHydrationWarning>
			<body>
				<Banner id="1.0-migration">
					🎉 @mysten/sui 1.0 has been released - Read the full migration guide here!
				</Banner>

				<RootProvider>{children}</RootProvider>
			</body>
		</html>
	);
}
