// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui, SuiLogoTxt } from '@mysten/icons';
import clsx from 'clsx';
import { useEffect, useState } from 'react';

import NetworkSelect from '../network/Network';
import Search from '../search/Search';
import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';
import { Heading } from '../../../../ui';

export function RedirectHeader() {
	return (
		<section
			className="mb-20 flex flex-col items-center justify-center gap-5 px-5 py-12 text-center"
			style={{
				background: 'linear-gradient(159deg, #FAF8D2 50.65%, #F7DFD5 86.82%)',
			}}
		>
			<div className="flex items-center gap-1">
				<Sui className="h-11 w-9" />
				<SuiLogoTxt className="h-7 w-11" />
			</div>

			<Heading variant="heading3/semibold">
				Experience two amazing blockchain explorers on Sui!
			</Heading>
		</section>
	);
}

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
				'flex h-header justify-center overflow-visible bg-white/40 backdrop-blur-xl transition-shadow',
				isScrolled && 'shadow-effect-ui-regular',
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
				<div className="flex w-full gap-2">
					<div className="flex-1">
						<Search />
					</div>
					<NetworkSelect />
				</div>
			</div>
		</header>
	);
}

export default Header;
