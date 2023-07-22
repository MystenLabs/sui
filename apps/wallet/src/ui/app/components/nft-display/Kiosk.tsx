// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getKioskIdFromDynamicFields, hasDisplayData, useGetKioskContents } from '@mysten/core';
import { getObjectDisplay, type SuiObjectResponse } from '@mysten/sui.js';
import cl from 'classnames';

import { NftImage, nftImageStyles, type NftImageProps } from './NftImage';
import { useActiveAddress } from '../../hooks';
import { Text } from '../../shared/text';

type KioskProps = {
	object: SuiObjectResponse;
	orientation?: 'vertical' | 'horizontal' | null;
} & Partial<NftImageProps>;

// this allows us to translate the first image of the kiosk display down without it
// overflowing from the bottom (and can be animated)
const clipPath = '[clip-path:inset(0_0_8px_0_round_12px)] group-hover:[clip-path:inset(0_0_0_0)]';
const timing = 'transition-all duration-250 ease-ease-out-cubic';
const cardStyles = [
	`scale-100 group-hover:scale-95 object-cover origin-bottom z-20 group-hover:translate-y-0 translate-y-2 group-hover:shadow-md`,
	'scale-[0.95] group-hover:-rotate-6 group-hover:-translate-x-6 group-hover:-translate-y-3 z-10 translate-y-0 group-hover:shadow-lg',
	'scale-[0.90] group-hover:rotate-6 group-hover:translate-x-6 group-hover:-translate-y-3 z-0 -translate-y-2 group-hover:shadow-xl',
];

function ItemCount({ count }: { count?: number }) {
	return (
		<div
			className={cl(
				timing,
				'right-1.5 bottom-1.5 flex items-center justify-center absolute h-6 w-6 bg-gray-100 text-white rounded-md',
				{ 'group-hover:-translate-x-0.5 group-hover:scale-95': count && count > 1 },
			)}
		>
			<Text variant="subtitle" weight="medium">
				{count}
			</Text>
		</div>
	);
}

export function Kiosk({ object, orientation, ...nftImageProps }: KioskProps) {
	const address = useActiveAddress();
	const { data: kioskData, isLoading } = useGetKioskContents(address);
	if (isLoading) return null;
	const kioskId = getKioskIdFromDynamicFields(object);
	const kiosk = kioskData?.kiosks.get(kioskId!);
	const hasItemsWithDisplay = kiosk?.items.some((item) => hasDisplayData(item));
	const items = kiosk?.items?.sort((item) => (hasDisplayData(item) ? -1 : 1)) ?? [];
	const showCardStackAnimation = items.length > 1 && orientation !== 'horizontal';
	const imagesToDisplay = orientation !== 'horizontal' ? 3 : 1;

	return (
		<div
			className={cl(
				'relative hover:bg-transparent group flex justify-between rounded-xl transform-gpu overflow-visible',
				nftImageStyles({ size: nftImageProps.size }),
			)}
		>
			<div className="absolute z-0">
				{!hasItemsWithDisplay ? (
					<NftImage animateHover src={null} name="Kiosk" {...nftImageProps} />
				) : items?.length ? (
					items.slice(0, imagesToDisplay).map((item, idx) => {
						const display = getObjectDisplay(item)?.data;
						return (
							<div
								className={cl(
									'absolute rounded-xl',
									timing,
									showCardStackAnimation ? cardStyles[idx] : '',
								)}
							>
								<div
									key={`${item.objectId} ${display?.image_url}`}
									className={`${idx === 0 && showCardStackAnimation ? clipPath : ''} ${timing}`}
								>
									<NftImage
										{...nftImageProps}
										src={display?.image_url!}
										animateHover={items.length <= 1}
										name="Kiosk"
									/>
								</div>
							</div>
						);
					})
				) : null}
			</div>
			{orientation !== 'horizontal' && <ItemCount count={items?.length} />}
		</div>
	);
}
