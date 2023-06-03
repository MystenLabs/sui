// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';
import FindKiosk from '../Kiosk/FindKiosk';
import { SuiConnectButton } from './SuiConnectButton';

export function Header(): JSX.Element {
  const navigate = useNavigate();

  return (
    <div className="border-b border-gray-400">
      <div className="md:flex items-center gap-5 container py-4 ">
        <button
          className="text-lg font-bold text-center mr-3 bg-transparent"
          onClick={() => navigate('/')}
        >
          Kiosk demo
        </button>
        <FindKiosk />
        <div className="ml-auto my-3 md:my-1">
          <SuiConnectButton></SuiConnectButton>
        </div>
      </div>
    </div>
  );
}
