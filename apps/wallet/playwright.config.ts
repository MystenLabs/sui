// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { devices } from '@playwright/test';
import type { PlaywrightTestConfig } from '@playwright/test';

/**
 * See https://playwright.dev/docs/test-configuration.
 */
const config: PlaywrightTestConfig = {
	testDir: './tests',
	/* Maximum time one test can run for. */
	timeout: 30 * 1000,
	expect: {
		/**
		 * Maximum time expect() should wait for the condition to be met.
		 * For example in `await expect(locator).toHaveText();`
		 */
		timeout: 5000,
	},
	/* Run tests in files in parallel */
	fullyParallel: true,
	/* Fail the build on CI if you accidentally left test.only in the source code. */
	forbidOnly: !!process.env.CI,
	/* Retry on CI only */
	retries: process.env.CI ? 2 : 0,
	/* Opt out of parallel tests on CI. */
	workers: process.env.CI ? 1 : undefined,
	/* Reporter to use. See https://playwright.dev/docs/test-reporters */
	reporter: 'html',
	/* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
	use: {
		/* Maximum time each action such as `click()` can take. Defaults to 0 (no limit). */
		actionTimeout: 0,
		/* Base URL to use in actions like `await page.goto('/')`. */
		// baseURL: 'http://localhost:3000',

		/* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
		trace: 'on-first-retry',
	},
	/* Configure projects for major browsers */
	projects: [
		{
			name: 'chromium',
			use: {
				...devices['Desktop Chrome'],
				userAgent: 'Playwright',
			},
		},
	],
	webServer: [
		{
			command: 'pnpm demoApp:dev',
			port: 5181,
			timeout: 30 * 1000,
			reuseExistingServer: !process.env.CI,
		},
		{
			command:
				process.env.E2E_RUN_LOCAL_NET_CMD ??
				'RUST_LOG="consensus=off" cargo run --bin sui -- start --with-faucet --force-regenesis',
			port: 9123,
			timeout: 120 * 1000,
			/* Set this to true to reuse the server instance on step 'Run Local net' in e2e.yml */
			reuseExistingServer: true,
		},
	],
};

export default config;
