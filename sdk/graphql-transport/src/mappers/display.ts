// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { DisplayFieldsResponse } from '@mysten/sui.js/client';

export function formatDisplay(object: {
	display?:
		| {
				key: string;
				value?: string | null | undefined;
				error?: string | null | undefined;
		  }[]
		| null;
}) {
	let display: DisplayFieldsResponse = {
		data: null,
		error: null,
	};

	if (object.display) {
		object.display.forEach((displayItem) => {
			if (displayItem.error) {
				display!.error = displayItem.error as never;
			} else if (displayItem.value != null) {
				if (!display!.data) {
					display!.data = {};
				}
				display!.data[displayItem.key] = displayItem.value;
			}
		});
	}

	return display;
}
