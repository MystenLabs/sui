// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  literal,
  string,
  tuple,
  number,
  integer,
  object,
  union,
  Infer,
} from 'superstruct';

export const CommitteeInfoResponse = object({
  epoch: integer(),
  committee_info: union([array(tuple([string(), number()])), literal(null)])
});

export type CommitteeInfoResponse = Infer<typeof CommitteeInfoResponse>;
