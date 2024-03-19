// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isErrorPayload, type Payload } from '_payloads';
import { lastValueFrom, map, take, type Observable } from 'rxjs';

export function mapToPromise<T extends Payload | void, R>(
	stream: Observable<T>,
	project: (value: T) => R,
) {
	return lastValueFrom(
		stream.pipe(
			take<T>(1),
			map<T, R>((response) => {
				if (response && isErrorPayload(response)) {
					// TODO: throw proper error
					throw new Error(response.message);
				}
				return project(response);
			}),
		),
	);
}
