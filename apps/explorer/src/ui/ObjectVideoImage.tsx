// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MediaPlay16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';

import { ObjectModal } from '~/ui/Modal/ObjectModal';
import { Image, type ImageProps } from '~/ui/image/Image';

const imageStyles = cva(['z-0 flex-shrink-0 relative'], {
	variants: {
		variant: {
			xs: 'h-8 w-8',
			small: 'h-16 w-16',
			medium: 'md:h-31.5 md:w-31.5 h-16 w-16',
			large: 'h-50 w-50',
			fill: 'h-full w-full',
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
	rounded?: ImageProps['rounded'];
	disablePreview?: boolean;
	fadeIn?: boolean;
	imgFit?: ImageProps['fit'];
	aspect?: ImageProps['aspect'];
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
	imgFit,
	aspect,
	rounded = 'md',
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
				<Image
					aspect={aspect}
					rounded={rounded}
					onClick={openPreview}
					alt={title}
					src={src}
					fadeIn={fadeIn}
					fit={imgFit}
				/>
				{video && (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80">
						<MediaPlay16 className={clsx(variant === 'large' ? 'h-8 w-8' : 'h-5 w-5')} />
					</div>
				)}
			</div>
		</>
	);
}
