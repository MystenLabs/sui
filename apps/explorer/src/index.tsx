// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';

import { router } from './pages';
import { loadFeatures } from './utils/growthbook';
import { plausible } from './utils/plausible';
import './utils/sentry';
import { reportWebVitals } from './utils/vitals';

import './index.css';

// Start loading features as early as we can:
loadFeatures();

// NOTE: The plausible tracker ensures it doesn't run on localhost, so we don't
// need to gate this call.
plausible.enableAutoPageviews();

ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
        <RouterProvider router={router} />
    </React.StrictMode>
);

reportWebVitals();
