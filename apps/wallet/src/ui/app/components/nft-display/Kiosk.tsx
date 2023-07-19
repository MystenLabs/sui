// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getKioskIdFromDynamicFields, hasDisplayData, useGetKioskContents } from '@mysten/core';
import { getObjectDisplay, type SuiObjectResponse } from '@mysten/sui.js';
import cl from 'classnames';

import { type NFTDisplayCardProps } from '.';
import { NftImage } from './NftImage';
import { useActiveAddress } from '../../hooks';
import { Text } from '../../shared/text';

type KioskProps = {
	object: SuiObjectResponse;
} & Partial<NFTDisplayCardProps>;

const styles: Record<number, string> = {
	0: 'scale-100 group-hover:scale-95 object-cover origin-bottom z-20 group-hover:shadow-blurXl group-hover:translate-y-0 translate-y-2 ',
	1: 'scale-[0.95] group-hover:-rotate-6 group-hover:-translate-x-6 group-hover:-translate-y-3 z-10 translate-y-0 group-hover:shadow-lg',
	2: 'scale-[0.90] group-hover:rotate-6 group-hover:translate-x-6 group-hover:-translate-y-3 z-0 -translate-y-2 group-hover:shadow-xl',
};

function ItemCount({ count }: { count?: number }) {
	return (
		<div className="right-1.5 bottom-1.5 transition-all flex items-center justify-center absolute h-6 w-6 bg-gray-100 text-white rounded-md">
			<Text variant="subtitle" weight="medium">
				{count}
			</Text>
		</div>
	);
}

export function Kiosk({ object, ...nftDisplayCardProps }: KioskProps) {
	const address = useActiveAddress();
	const { data: kioskData, isLoading } = useGetKioskContents(address);

	if (isLoading) return null;

	const kioskId = getKioskIdFromDynamicFields(object);
	const kiosk = kioskData?.kiosks.get(kioskId!);
	const hasItemsWithDisplay = kiosk?.items.some((item) => hasDisplayData(item));
	const items = kiosk?.items?.sort((item) => (hasDisplayData(item) ? -1 : 1));

	return (
		<div className="relative hover:bg-transparent group flex justify-between h-36 w-36 rounded-xl transform-gpu overflow-hidden group-hover:overflow-visible transition-all">
			<div className="absolute z-0">
				{!hasItemsWithDisplay ? (
					<NftImage showLabel animateHover src={null} name="Kiosk" {...nftDisplayCardProps} />
				) : items?.length ? (
					items.slice(0, 3).map((item, idx) => {
						const display = getObjectDisplay(item)?.data;
						return (
							<div
								key={`${item.objectId} ${display?.image_url}`}
								className={cl(
									items.length > 1 ? styles[idx] : 'group-hover:scale-105',
									'absolute transition-all ease-ease-out-cubic duration-250 rounded-xl ',
								)}
							>
								<NftImage
									src={display?.image_url!}
									borderRadius={nftDisplayCardProps.borderRadius}
									size={nftDisplayCardProps.size}
									animateHover={items.length <= 1}
									name="Kiosk"
								/>
							</div>
						);
					})
				) : null}
			</div>
			<ItemCount count={items?.length} />
		</div>
	);
}
