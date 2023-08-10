// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MediaPlay16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';

import { ObjectModal } from '~/ui/Modal/ObjectModal';
import { Image } from '~/ui/image/Image';

const imageStyles = cva(['cursor-pointer z-0 flex-shrink-0 relative'], {
	variants: {
		variant: {
			xs: 'h-8 w-8',
			small: 'h-16 w-16',
			large: 'h-50 w-50',
		},
	},
});

type ImageStylesProps = VariantProps<typeof imageStyles>;

interface Props extends ImageStylesProps {
	title: string;
	subtitle: string;
	open: boolean;
	setOpen: (open: boolean) => void;
	src: string;
	video?: string | null;
}

export function ObjectVideoImage({ title, subtitle, src, video, variant, open, setOpen }: Props) {
	const close = () => setOpen(false);
	const openPreview = () => setOpen(true);

	return (
		<>
			<ObjectModal
				open={open}
				onClose={close}
				title={title}
				subtitle={subtitle}
				src={src}
				video={video}
				alt={title}
			/>
			<div className={imageStyles({ variant })}>
				<Image rounded="md" onClick={openPreview} alt={title} src={src} />
				{video && (
					<div className="pointer-events-none absolute bottom-2 right-2 z-10 flex items-center justify-center rounded-full opacity-80">
						<MediaPlay16 className={clsx(variant === 'large' ? 'h-8 w-8' : 'h-5 w-5')} />
					</div>
				)}
			</div>
		</>
	);
}
