// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner } from '@mysten/sui/client';
import { ReactNode } from 'react';

import { ObjectLink } from './ObjectLink';

type HeaderProps = {
	children?: ReactNode;
};
type RootProps = {
	children: ReactNode;
	className?: string;
};

type BodyProps = {
	children: ReactNode;
};

type FooterProps = {
	children?: ReactNode;
};

function Root({ children, className }: RootProps) {
	return (
		<div className={`border flex flex-col  rounded-lg shadow overflow-hidden ${className}`}>
			{children}
		</div>
	);
}

function Body({ children }: BodyProps) {
	return <div className="p-3 overflow-x-auto">{children}</div>;
}

function Header({ children }: HeaderProps) {
	return (
		<div className="bg-gray-900 py-3 px-2 text-sm overflow-x-auto break-words">{children}</div>
	);
}
function Footer({ children, owner }: FooterProps & { owner?: ObjectOwner }) {
	return (
		<div className="mt-auto bg-gray-900 py-3 px-2 text-sm overflow-x-auto break-words">
			{children}
			{owner && (
				<div className="flex items-center ">
					<div>Owner</div>
					<div className="col-span-3 text-right flex items-center gap-1 ml-auto">
						<ObjectLink owner={owner} />
					</div>
				</div>
			)}
		</div>
	);
}

export const PreviewCard = {
	Root,
	Header,
	Body,
	Footer,
};
