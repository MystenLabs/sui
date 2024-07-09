// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useSuiClientContext } from '@mysten/dapp-kit';
import { ObjectOwner, SuiObjectChange } from '@mysten/sui/client';
import { CheckIcon, CopyIcon } from 'lucide-react';
import { useState } from 'react';
import toast from 'react-hot-toast';

import { formatAddress } from './utils';

type OwnerDisplay = string | { address: string } | { object: string };

const getOwnerDisplay = (owner: ObjectOwner): OwnerDisplay => {
	if (owner === 'Immutable') return 'Immutable';
	if ('Shared' in owner) return 'Shared';
	if ('AddressOwner' in owner) return { address: owner.AddressOwner };
	return { object: owner.ObjectOwner };
};

export function ObjectLink({
	owner,
	type,
	object,
	inputObject,
	...tags
}: {
	inputObject?: string;
	type?: string;
	owner?: ObjectOwner;
	object?: SuiObjectChange;
} & React.HTMLAttributes<HTMLAnchorElement> &
	React.ComponentPropsWithoutRef<'a'>) {
	const [copied, setCopied] = useState(false);

	const { network } = useSuiClientContext();

	let objectId: string | undefined;
	let display: string | undefined;

	const ownerDisplay = owner ? getOwnerDisplay(owner) : undefined;

	if (ownerDisplay) {
		if (typeof ownerDisplay !== 'string') {
			objectId = 'address' in ownerDisplay ? ownerDisplay.address : ownerDisplay.object;
			display = formatAddress(objectId);
		} else {
			display = ownerDisplay;
		}
	}

	if (type) {
		display = type;
	}

	if (inputObject) {
		objectId = inputObject;
		display = formatAddress(inputObject);
	}

	if (object) {
		if ('objectId' in object) {
			objectId = object.objectId;
			display = formatAddress(objectId);
		}

		if ('packageId' in object) {
			objectId = object.packageId;
			display = formatAddress(objectId);
		}
	}

	const link = objectId
		? `https://suiexplorer.com/${ownerDisplay ? 'address' : 'object'}/${objectId}?network=${
				network.split(':')[1]
			}`
		: undefined;

	const copy = () => {
		if (!objectId && !display) return;

		navigator.clipboard.writeText(objectId || display || '');
		setCopied(true);
		toast.success('Copied to clipboard!');

		setTimeout(() => {
			setCopied(false);
		}, 1_000);
	};

	return (
		<>
			{copied ? (
				<CheckIcon width={10} height={10} className="" />
			) : display ? (
				<CopyIcon width={10} height={10} className="cursor-pointer" onClick={copy} />
			) : null}

			{link ? (
				<>
					<a
						href={link}
						target="_blank"
						className="underline break-words pl-2"
						{...tags}
						rel="noreferrer"
					>
						{display}
					</a>
				</>
			) : (
				<span>{display || '-'}</span>
			)}
		</>
	);
}
