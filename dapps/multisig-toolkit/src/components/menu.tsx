// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { NavLink } from 'react-router-dom';

const links = [
	{ to: '/offline-signer', label: 'Offline Signer' },
	{ to: '/signature-analyzer', label: 'Signature Analyzer' },
	{ to: '/multisig-address', label: 'MultiSig Address' },
	{ to: '/combine-signatures', label: 'Combine MultiSig Signatures' },
	{ to: '/execute-transaction', label: 'Execute Transaction' },
	{ to: '/help', label: 'Help' },
];

export function Menu({ callback }: { callback?: () => void }) {
	return (
		<>
			{links.map(({ to, label }) => (
				<NavLink
					key={to}
					to={to}
					className={({ isActive }) =>
						isActive
							? 'text-sm font-semibold transition-colors hover:text-primary'
							: 'text-sm font-semibold text-muted-foreground transition-colors hover:text-primary'
					}
					onClick={() => (callback ? callback() : null)}
				>
					{label}
				</NavLink>
			))}
		</>
	);
}
