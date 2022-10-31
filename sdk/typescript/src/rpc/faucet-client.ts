// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fetch from 'cross-fetch';

import { FaucetResponse, SuiAddress } from '../types';
import { HttpHeaders } from './client';

export async function requestSuiFromFaucet(
  endpoint: string,
  recipient: SuiAddress,
  httpHeaders?: HttpHeaders
): Promise<FaucetResponse> {
  const res = await fetch(endpoint, {
    method: 'POST',
    body: JSON.stringify({
      FixedAmountRequest: {
        recipient,
      },
    }),
    headers: {
      'Content-Type': 'application/json',
      ...(httpHeaders || {}),
    },
  });
  const parsed = await res.json();

  if (parsed.error) {
    throw new Error(`Faucet returns error: ${parsed.error}`);
  }
  return parsed;
}
