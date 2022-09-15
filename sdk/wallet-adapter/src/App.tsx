// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import './App.css';
import { root } from '.';
import { Wallet, WalletProvider } from '@mysten/wallet-adapter-react';
import { SuiWalletAdapter, MockWalletAdapter} from '@mysten/wallet-adapter-all-wallets';
import { WalletWrapper } from '@mysten/wallet-adapter-react-ui';
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
