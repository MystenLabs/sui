// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { Placeholder } from '@mysten/ui';
import { type ReactNode } from 'react';

import OwnedObject from '~/components/OwnedObjects/OwnedObject';

interface Props {
	data?: SuiObjectResponse[];
	loading?: boolean;
}

function OwnObjectContainer({ children }: { children: ReactNode }) {
	return (
		<div className="w-full min-w-ownObjectContainer pb-3 pr-4 sm:basis-1/2 md:basis-1/3">
			<div className="rounded-lg p-2 hover:bg-hero/5">{children}</div>
		</div>
	);
}

function SmallThumbNailsViewLoading() {
	return (
		<>
			{new Array(10).fill(0).map((_, index) => (
				<OwnObjectContainer key={index}>
					<Placeholder rounded="lg" height="66px" />
				</OwnObjectContainer>
			))}
		</>
	);
}

export function SmallThumbNailsView({ data, loading }: Props) {
	return (
		<div className="flex h-full flex-row flex-wrap overflow-x-hidden">
			{loading && <SmallThumbNailsViewLoading />}
			{data?.map((obj, index) => (
				<OwnObjectContainer key={index}>
					<OwnedObject obj={obj} key={obj?.data?.objectId} />
				</OwnObjectContainer>
			))}
		</div>
	);
}
