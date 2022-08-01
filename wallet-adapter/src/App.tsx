// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import './App.css';
import { root } from '.';
import { Wallet, WalletProvider } from 'sui-wallet-adapter-react';
import { SuiWalletAdapter, MockWalletAdapter} from '@sui-wallet-adapter/all-wallets';
import { WalletWrapper } from 'sui-wallet-adapter-ui';
import { Button } from '@mui/material';
import { TestButton } from './TestButton';

function App() {
  const supportedWallets: Wallet[] = [
    {
      adapter: new SuiWalletAdapter()
    },
  ];

  return (
    <div className="App">
      <header className="App-header">
         <WalletProvider supportedWallets={supportedWallets}>
          <TestButton/>
          <WalletWrapper/>
        </WalletProvider>
      </header>
    </div>
  );
}

export default App;