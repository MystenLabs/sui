// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui, SuiLogoTxt } from '@mysten/icons';
import clsx from 'clsx';
import { useEffect, useState } from 'react';

import NetworkSelect from '../network/Network';
import Search from '../search/Search';
import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

function Header() {
	const [isScrolled, setIsScrolled] = useState(window.scrollY > 0);
	useEffect(() => {
		const callback = () => {
			setIsScrolled(window.scrollY > 0);
		};
		document.addEventListener('scroll', callback, { passive: true });
		return () => {
			document.removeEventListener('scroll', callback);
		};
	}, []);
	return (
		<header
			className={clsx(
				'sticky top-0 z-20 flex h-header justify-center overflow-visible bg-white/40 backdrop-blur-xl transition-shadow',
				isScrolled && 'shadow-mistyEdge',
			)}
		>
			<div className="flex h-full max-w-[1440px] flex-1 items-center gap-5 px-5 2xl:p-0">
				<LinkWithQuery
					data-testid="nav-logo-button"
					to="/"
					className="flex flex-nowrap items-center gap-1 text-hero-darkest"
				>
					<Sui className="h-[26px] w-5" />
					<SuiLogoTxt className="h-[17px] w-[27px]" />
				</LinkWithQuery>
				<div className="flex-1">
					<Search />
				</div>
				<NetworkSelect />
			</div>
		</header>
	);
}

export default Header;
