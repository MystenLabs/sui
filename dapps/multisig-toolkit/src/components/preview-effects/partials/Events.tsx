// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiEvent } from '@mysten/sui/client';
import { ReactNode } from 'react';

import { Textarea } from '@/components/ui/textarea';

import { ObjectLink } from '../ObjectLink';
import { PreviewCard } from '../PreviewCard';

export function Events({ events }: { events: SuiEvent[] }) {
	if (events.length === 0) {
		return <div>No events were emitted.</div>;
	}

	return (
		<div>
			{events.map((event, index) => (
				<Event key={index} event={event} />
			))}
		</div>
	);
}

export function Event({ event }: { event: SuiEvent }) {
	const fields: Record<string, ReactNode> = {
		'Package ID': <ObjectLink inputObject={event.packageId} />,
		Sender: <ObjectLink owner={{ AddressOwner: event.sender }} />,
		Data: event.parsedJson ? (
			<Textarea value={JSON.stringify(event.parsedJson, null, 2)} rows={6} readOnly />
		) : (
			'-'
		),
	};

	return (
		<PreviewCard.Root>
			<PreviewCard.Header>
				<p>
					Event Type: <strong>{event.type}</strong>
				</p>
			</PreviewCard.Header>
			<PreviewCard.Body>
				{Object.entries(fields).map(([key, value]) => (
					<div key={key} className="flex items-center gap-3 mb-3 ">
						<p className="capitalize min-w-[100px]">{key}: </p>
						{value}
					</div>
				))}
			</PreviewCard.Body>
		</PreviewCard.Root>
	);
}
