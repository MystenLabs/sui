// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight16 } from '@mysten/icons';
import clsx from 'clsx';
import type { MouseEventHandler, ReactNode } from 'react';
import { Link } from 'react-router-dom';

export type ItemProps = {
	icon: ReactNode;
	title: ReactNode;
	subtitle?: ReactNode;
	iconAfter?: ReactNode;
	to?: string;
	href?: string;
	onClick?: MouseEventHandler<Element>;
};

function MenuListItem({
	icon,
	title,
	subtitle,
	iconAfter,
	to = '',
	href = '',
	onClick,
}: ItemProps) {
	const Component = to ? Link : 'div';

	const MenuItemContent = (
		<>
			<div className="flex flex-nowrap flex-1 gap-2 items-center overflow-hidden basis-3/5">
				<div className="flex text-steel text-2xl flex-none">{icon}</div>
				<div className="flex-1 text-gray-90 text-body font-semibold flex">{title}</div>
			</div>
			{subtitle || iconAfter || to ? (
				<div
					className={clsx(
						{ 'flex-1 basis-2/5': Boolean(subtitle) },
						'flex flex-nowrap justify-end gap-1 items-center overflow-hidden',
					)}
				>
					{subtitle ? (
						<div className="transition text-steel-dark text-bodySmall font-medium group-hover:text-steel-darker">
							{subtitle}
						</div>
					) : null}
					<div className="transition flex text-steel flex-none text-base group-hover:text-steel-darker">
						{iconAfter || (to && <ChevronRight16 />) || null}
					</div>
				</div>
			) : null}
		</>
	);

	if (href) {
		return (
			<a
				href={href}
				target="_blank"
				rel="noreferrer noopener"
				className="flex flex-nowrap items-center px-1 py-4.5 first:pt-3 first:pb-3 last:pb-3 gap-5 no-underline overflow-hidden group cursor-pointer"
			>
				{MenuItemContent}
			</a>
		);
	}
	return (
		<Component
			data-testid={title}
			className="flex flex-nowrap items-center px-1 py-5 first:pt-3 first:pb-3 last:pb-3 gap-5 no-underline overflow-hidden group cursor-pointer"
			to={to}
			onClick={onClick}
		>
			{MenuItemContent}
		</Component>
	);
}

export default MenuListItem;
