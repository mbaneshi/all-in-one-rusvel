import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { defineConfig, devices } from '@playwright/test';

const API_PORT = 3000;
const DEV_PORT = 5173;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const isCi = process.env.CI === 'true';

const apiWebServer = {
	command: isCi ? path.join(repoRoot, 'target', 'debug', 'rusvel') : 'cargo run',
	cwd: isCi ? repoRoot : '..',
	port: API_PORT,
	timeout: isCi ? 60_000 : 120_000,
	reuseExistingServer: true
};

export default defineConfig({
	globalSetup: './tests/global-setup.ts',
	testDir: './tests',
	outputDir: './test-results',
	snapshotDir: './tests/visual-baselines',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	workers: process.env.CI ? 1 : undefined,
	reporter: [['html', { open: 'never' }], ['json', { outputFile: 'test-results/results.json' }]],

	use: {
		baseURL: `http://localhost:${DEV_PORT}`,
		screenshot: 'on',
		trace: 'on-first-retry',
		actionTimeout: 10_000
	},

	projects: [
		{
			name: 'visual',
			testMatch: '*.visual.ts',
			use: {
				...devices['Desktop Chrome'],
				viewport: { width: 1280, height: 720 }
			}
		},
		{
			name: 'e2e',
			testMatch: '*.e2e.ts',
			use: {
				...devices['Desktop Chrome'],
				viewport: { width: 1280, height: 720 }
			}
		}
	],

	webServer: [
		apiWebServer,
		{
			command: 'pnpm dev',
			port: DEV_PORT,
			timeout: 30_000,
			reuseExistingServer: true
		}
	]
});
