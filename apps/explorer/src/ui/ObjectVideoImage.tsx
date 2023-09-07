// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MediaPlay16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';

import { ObjectModal } from '~/ui/Modal/ObjectModal';
import { Image } from '~/ui/image/Image';

const imageStyles = cva(['z-0 flex-shrink-0 relative'], {
	variants: {
		variant: {
			xs: 'h-8 w-8',
			small: 'h-16 w-16',
			medium: 'md:h-31.5 md:w-31.5 h-16 w-16',
			large: 'h-50 w-50',
		},
		disablePreview: {
			true: '',
			false: 'cursor-pointer',
		},
	},
	defaultVariants: {
		disablePreview: false,
	},
});

type ImageStylesProps = VariantProps<typeof imageStyles>;

interface Props extends ImageStylesProps {
	title: string;
	subtitle: string;
	src: string;
	open?: boolean;
	setOpen?: (open: boolean) => void;
	video?: string | null;
	disablePreview?: boolean;
	fadeIn?: boolean;
}

export function ObjectVideoImage({
	title,
	subtitle,
	src,
	video,
	variant,
	open,
	setOpen,
	disablePreview,
	fadeIn,
}: Props) {
	const close = () => {
		if (disablePreview) {
			return;
		}

		if (setOpen) {
			setOpen(false);
		}
	};
	const openPreview = () => {
		if (disablePreview) {
			return;
		}

		if (setOpen) {
			setOpen(true);
		}
	};

	return (
		<>
			<ObjectModal
				open={!!open}
				onClose={close}
				title={title}
				subtitle={subtitle}
				src={src}
				video={video}
				alt={title}
			/>
			<div className={imageStyles({ variant, disablePreview })}>
				<Image rounded="md" onClick={openPreview} alt={title} src={src} fadeIn={fadeIn} />
				{video && (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80">
						<MediaPlay16 className={clsx(variant === 'large' ? 'h-8 w-8' : 'h-5 w-5')} />
					</div>
				)}
			</div>
		</>
	);
}
