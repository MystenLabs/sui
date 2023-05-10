// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import { FeatureFlags } from '@mysten/core';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';

import { router } from './pages';
import { growthbook } from './utils/growthbook';
import './utils/sentry';
import { reportWebVitals } from './utils/vitals';

import './index.css';

const firebaseConfig = {
    apiKey: 'AIzaSyBv7OTAN5peimooJoardhDgnP2PH-yh8lI',
    authDomain: 'test-477b4.firebaseapp.com',
    projectId: 'test-477b4',
    storageBucket: 'test-477b4.appspot.com',
    messagingSenderId: '347511764393',
    appId: '1:347511764393:web:df238258992a948063c0b8',
};

const flags = new FeatureFlags(firebaseConfig);

console.log({
    a: flags.getFeature('test'),
    b: flags.getFeature('test'),
});

flags.ready.then((fetche) => {
    console.log(fetche);
    console.log({
        a2: flags.getFeature('test'),
        b2: flags.getFeature('test'),
    });
});

// Start loading features as early as we can:
growthbook.loadFeatures();

ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
        <RouterProvider router={router} />
    </React.StrictMode>
);

reportWebVitals();
