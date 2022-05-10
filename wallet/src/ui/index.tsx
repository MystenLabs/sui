// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createRoot } from 'react-dom/client';

import App from './app';

import './styles/global.scss';

const rootDom = document.getElementById('root');
if (!rootDom) {
    throw new Error('Root element not found');
}
const root = createRoot(rootDom);
root.render(<App />);
