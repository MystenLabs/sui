// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RootToggle } from 'fumadocs-ui/components/layout/root-toggle';
import { DocsLayout } from 'fumadocs-ui/layout';
import type { ReactNode } from 'react';

import { docsOptions } from '../layout.config';

export default function Layout({ children }: { children: ReactNode }) {
	return (
		<DocsLayout
			{...docsOptions}
			sidebar={{
				banner: (
					<RootToggle
						options={[
							{
								icon: null,
								title: 'TypeScript SDK',
								description: 'Core Sui TypeScript SDK',
								url: '/typescript',
							},
							{
								icon: null,
								title: 'dApp Kit',
								description: 'React Hooks and Components',
								url: '/dapp-kit',
							},
							{
								icon: null,
								title: 'Kiosk SDK',
								description: 'TODO: Add description',
								url: '/kiosk',
							},
							{
								icon: null,
								title: 'zkSend SDK',
								description: 'TODO: Add description',
								url: '/zksend',
							},
							{
								icon: null,
								title: 'BCS',
								description: 'TODO: Add description',
								url: '/bcs',
							},
						]}
					/>
				),
			}}
		>
			{children}
		</DocsLayout>
	);
}
