// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PreviewCard } from '../preview-effects/PreviewCard';
import { EffectsObject, Reference, ReplayInput } from './replay-types';
import { ReplayLink } from './ReplayLink';

const formatObject = (input: ReplayInput) => {
	if (!input || !('Object' in input) || !input.Object) return input;
	const data = input.Object;

	if ('SharedObject' in data && data.SharedObject) {
		return {
			objectId: data.SharedObject.id,
			mutable: data.SharedObject.mutable,
			initialSharedVersion: data.SharedObject.initialSharedVersion,
		};
	}

	if ('ImmOrOwnedObject' in data && data.ImmOrOwnedObject) {
		return {
			objectId: data.ImmOrOwnedObject[0],
			version: data.ImmOrOwnedObject[1],
		};
	}

	return data;
};

export function ReplayInputArgument({ input }: { input: ReplayInput }) {
	return (
		<PreviewCard.Root>
			<PreviewCard.Body>
				{'Pure' in input && <p>Pure: {JSON.stringify(input.Pure)}</p>}
				{'Object' in input && (
					<>
						{Object.entries(formatObject(input)).map(([key, value]) => (
							<div>
								<span className="capitalize">{key}: </span>
								{key === 'objectId' && <ReplayLink id={value} text={value} />}
								{key !== 'objectId' && <span>{JSON.stringify(value)}</span>}
							</div>
						))}
					</>
				)}
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
}

export function EffectsItem({ input }: { input: EffectsObject }) {
	return (
		<PreviewCard.Root>
			<PreviewCard.Body>
				{<ReplayLink id={input.reference.objectId} text={input.reference.objectId} />}
				{'ObjectOwner' in input.owner && <p>Object Owner: {input.owner.ObjectOwner}</p>}
				{'AddressOwner' in input.owner && <p>Address Owner: {input.owner.AddressOwner}</p>}
				{'Shared' in input.owner && <p>Shared Object</p>}
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
}

export function DeletedItem({ input }: { input: Reference }) {
	return (
		<PreviewCard.Root>
			<PreviewCard.Body>
				{<ReplayLink id={input.objectId} text={input.objectId} />}
				<div>version: {<span>{JSON.stringify(input.version)}</span>}</div>
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
}
