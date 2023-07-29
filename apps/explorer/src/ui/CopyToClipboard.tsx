// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCopyToClipboard } from '@mysten/core';
import { Check12, CheckStroke16, CheckStroke24, Copy12, Copy16, Copy24 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import { motion } from 'framer-motion';
import { useEffect, useState } from 'react';
import { toast } from 'react-hot-toast';

import { Link } from '~/ui/Link';

const iconStyles = cva([], {
	variants: {
		size: {
			sm: 'w-3 h-3',
			md: 'w-4 h-4',
			lg: 'w-4 h-4 md:w-6 md:h-6',
		},
		color: {
			gray45: 'text-gray-45',
			steel: 'text-steel',
		},
		success: {
			true: 'text-success',
			false: 'hover:text-steel-dark cursor-pointer',
		},
	},
	defaultVariants: {
		size: 'md',
		color: 'gray45',
	},
});

export type IconStylesProps = VariantProps<typeof iconStyles>;

export interface CopyToClipboardProps extends Omit<IconStylesProps, 'success'> {
	copyText: string;
	onSuccessMessage?: string;
}

const COPY_ICON_SIZES = {
	sm: Copy12,
	md: Copy16,
	lg: Copy24,
};

const CHECK_ICON_SIZES = {
	sm: Check12,
	md: CheckStroke16,
	lg: CheckStroke24,
};

const TIMEOUT_TIMER = 2000;

export function CopyToClipboard({
	copyText,
	color,
	size = 'md',
	onSuccessMessage = 'Copied!',
}: CopyToClipboardProps) {
	const [copied, setCopied] = useState(false);
	const copyToClipBoard = useCopyToClipboard(() => toast.success(onSuccessMessage));

	const CopyIcon = COPY_ICON_SIZES[size!];
	const CheckIcon = CHECK_ICON_SIZES[size!];

	const handleCopy = async () => {
		await copyToClipBoard(copyText);
		setCopied(true);
	};

	useEffect(() => {
		if (copied) {
			const timeout = setTimeout(() => {
				setCopied(false);
			}, TIMEOUT_TIMER);

			return () => clearTimeout(timeout);
		}
	}, [copied]);

	return (
		<Link disabled={copied} onClick={handleCopy}>
			<span className="sr-only">Copy</span>
			{copied ? (
				<CheckIcon className={iconStyles({ size, color, success: true })} />
			) : (
				<motion.div
					initial={{ opacity: 0 }}
					animate={{ opacity: 1 }}
					transition={{ duration: 0.2 }}
				>
					<CopyIcon className={iconStyles({ size, color, success: false })} />
				</motion.div>
			)}
		</Link>
	);
}
