// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Heading } from '_src/ui/app/shared/heading';
import { cva, type VariantProps } from 'class-variance-authority';
import { type AnchorHTMLAttributes, type ElementType, type ReactNode } from 'react';

const cardStyles = cva(
	['bg-white relative flex flex-col p-4.5 w-full shadow-card-soft rounded-2xl'],
	{
		variants: {
			as: {
				div: '',
				a: 'no-underline text-hero-dark hover:text-hero visited:text-hero-dark',
			},
		},
	},
);

interface Props extends VariantProps<typeof cardStyles> {
	heading?: string;
	after?: ReactNode;
	children: ReactNode;
	footer?: ReactNode;
}

type CardProps = Props & AnchorHTMLAttributes<HTMLAnchorElement>;

export const SummaryCardFooter = ({ children }: { children: ReactNode }) => {
	return (
		<div className="-mb-4.5 -mx-4.5 px-4 py-2 rounded-b-2xl flex justify-between items-center bg-sui/10 ">
			{children}
		</div>
	);
};

export function Card({ as = 'div', heading, children, after, footer = null, ...props }: CardProps) {
	const Component = as as ElementType;
	return (
		<Component className={cardStyles({ as })} {...props}>
			{heading && (
				<div className="flex items-center justify-between mb-4 last-of-type:mb-0">
					<Heading variant="heading6" color="steel-darker">
						{heading}
					</Heading>
					{after && <div>{after}</div>}
				</div>
			)}
			{children}
			{footer}
		</Component>
	);
}
