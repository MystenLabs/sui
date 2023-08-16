// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mitt from 'mitt';

type AccountsEvents = {
	accountsChanged: void;
	accountStatusChanged: { accountID: string };
	activeAccountChanged: { accountID: string };
};

export const accountsEvents = mitt<AccountsEvents>();
