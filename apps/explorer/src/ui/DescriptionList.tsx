// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

import type { ReactNode } from 'react';

const descriptionItemStyles = cva(['flex flex-col gap-2 md:flex-row md:gap-10'], {
	variants: {
		align: {
			start: 'md:items-start',
			center: 'md:items-center',
		},
	},
	defaultVariants: {
		align: 'center',
	},
});

const descriptionItemLabelStyles = cva(
	['w-full flex flex-shrink-0 text-pBody font-medium text-steel-darker'],
	{
		variants: {
			labelWidth: {
				sm: 'md:w-28',
				md: 'md:w-40',
				lg: 'md:w-48',
			},
		},
		defaultVariants: {
			labelWidth: 'md',
		},
	},
);

const descriptionItemDescriptionStyles = cva(['ml-0 min-w-0 flex-1 leading-none'], {
	variants: {
		descriptionJustify: {
			end: 'flex justify-end',
		},
	},
});

type DescriptionItemStylesProps = VariantProps<typeof descriptionItemStyles>;
type DescriptionItemLabelStylesProps = VariantProps<typeof descriptionItemLabelStyles>;
type DescriptionItemDescriptionStylesProps = VariantProps<typeof descriptionItemDescriptionStyles>;

export interface DescriptionItemProps
	extends DescriptionItemStylesProps,
		DescriptionItemLabelStylesProps,
		DescriptionItemDescriptionStylesProps {
	title: string | ReactNode;
	children: ReactNode;
}

export function DescriptionItem({
	title,
	align,
	labelWidth,
	descriptionJustify,
	children,
}: DescriptionItemProps) {
	return (
		<div className={descriptionItemStyles({ align })}>
			<dt className={descriptionItemLabelStyles({ labelWidth })}>{title}</dt>
			<dd className={descriptionItemDescriptionStyles({ descriptionJustify })}>{children}</dd>
		</div>
	);
}

export type DescriptionListProps = {
	children: ReactNode;
};

export function DescriptionList({ children }: DescriptionListProps) {
	return <dl className="mt-4 flex flex-col gap-4">{children}</dl>;
}
