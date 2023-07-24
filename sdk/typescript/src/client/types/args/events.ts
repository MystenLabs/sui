// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiJsonValue } from '../common.js';

export type MoveEventField = {
	path: string;
	value: SuiJsonValue;
};

// mirrors sui_json_rpc_types::SuiEventFilter
export type SuiEventFilter =
	| { Package: string }
	| { MoveModule: { package: string; module: string } }
	| { MoveEventType: string }
	| { MoveEventField: MoveEventField }
	| { Transaction: string }
	| {
			TimeRange: {
				// left endpoint of time interval, milliseconds since epoch, inclusive
				startTime: string;
				// right endpoint of time interval, milliseconds since epoch, exclusive
				endTime: string;
			};
	  }
	| { Sender: string }
	| { All: SuiEventFilter[] }
	| { Any: SuiEventFilter[] }
	| { And: [SuiEventFilter, SuiEventFilter] }
	| { Or: [SuiEventFilter, SuiEventFilter] };
