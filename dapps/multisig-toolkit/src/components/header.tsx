// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { KeyRound, MenuIcon, XIcon } from 'lucide-react';
import { useState } from 'react';
import { NavLink } from 'react-router-dom';

import { Menu } from './menu';

export function Header() {
	const [showMobileMenu, setShowMobileMenu] = useState(false);

	return (
		<div className="border-b px-8 py-4 flex items-center justify-between">
			<NavLink to="/">
				<div className="flex items-center gap-2">
					<KeyRound strokeWidth={2} size={18} className="text-primary/80" />
					<h1 className="font-bold text-lg bg-clip-text text-transparent bg-gradient-to-r from-primary to-primary/60">
						Sui MultiSig Toolkit
					</h1>
				</div>
			</NavLink>

			<div className="max-lg:hidden flex gap-4">
				<Menu />
			</div>

			<div className="lg:hidden">
				<MenuIcon onClick={() => setShowMobileMenu(true)} />
			</div>

			{showMobileMenu && (
				<div className="lg:hidden fixed inset-0 bg-background z-50">
					<div className="absolute top-0 right-0 p-4">
						<XIcon onClick={() => setShowMobileMenu(false)} />
					</div>
					<div className="grid grid-cols-1 gap-5 text-xl md:text-2xl px-6 py-12">
						<Menu callback={() => setShowMobileMenu(false)} />
					</div>
				</div>
			)}
		</div>
	);
}
