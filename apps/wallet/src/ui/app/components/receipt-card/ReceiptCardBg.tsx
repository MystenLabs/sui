// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';
import type { ReactNode } from 'react';

// TODO: use a single svg and change background color instead of using multiple svgs
const backgroundStyles = cva(
	[
		"p-5 pb-0 rounded-t-4lg flex flex-col item-center after:content-[''] after:w-[320px] after:h-5 after:ml-[-20px] after:top-4 after:-mt-6 after:relative divide-y divide-solid divide-steel/20 divide-x-0",
	],
	{
		variants: {
			status: {
				success: "bg-success-light after:bg-[url('_assets/images/receipt_bottom.svg')]",
				failure: "bg-issue-light after:bg-[url('_assets/images/receipt_bottom_red.svg')]",
				pending: "bg-warning-light after:bg-[url('_assets/images/receipt_bottom_yellow.svg')]",
			},
		},
	},
);

export interface ReceiptCardBgProps extends VariantProps<typeof backgroundStyles> {
	children: ReactNode;
}

export function ReceiptCardBg({ children, ...styleProps }: ReceiptCardBgProps) {
	return <div className={backgroundStyles(styleProps)}>{children}</div>;
}
