// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'cypress';

export default defineConfig({
    e2e: {
        async setupNodeEvents(on, _config) {
            const { createLocalnetTasks } = await import('./cypress/localnet');
            on('task', await createLocalnetTasks());
        },
    },
    component: {
        devServer: {
            framework: 'react',
            bundler: 'vite',
        },
    },
});
