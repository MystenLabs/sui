// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletKitProvider } from '@mysten/wallet-kit';
import React from 'react';
import ReactDOM from 'react-dom/client';

import { App } from './App';

import './index.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
	<React.StrictMode>
		<WalletKitProvider enableUnsafeBurner>
			<App />
		</WalletKitProvider>
	</React.StrictMode>,
);
