/**
 * Shared test fixtures: injects the test session into the SvelteKit app
 * via localStorage before each test.
 */

import { test as base, type Page } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'node:url';
import type { TestState } from './global-setup';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.join(__dirname, '..', 'test-results', '.test-state.json');

function loadTestState(): TestState {
	return JSON.parse(fs.readFileSync(STATE_FILE, 'utf-8'));
}

/**
 * Inject the active session into the app's Svelte store via localStorage/fetch,
 * then navigate to the given route and wait for the page to settle.
 */
export async function setupSession(page: Page) {
	const state = loadTestState();

	// Navigate to root first to establish the origin
	await page.goto('/');

	// Set the active session in localStorage (the app reads this on mount)
	await page.evaluate(
		({ sessionId, sessionName }) => {
			// The app's stores.ts reads activeSession from a fetch to /api/sessions,
			// so we prime localStorage with the session data for the onboarding store
			localStorage.setItem(
				'rusvel-onboarding',
				JSON.stringify({
					sessionCreated: true,
					goalAdded: true,
					planGenerated: false,
					deptChatUsed: false,
					agentCreated: false,
					dismissed: true,
					tourCompleted: true
				})
			);
			// Store active session ID for the app to pick up
			localStorage.setItem('rusvel-active-session', sessionId);
		},
		{ sessionId: state.sessionId, sessionName: state.sessionName }
	);
}

/**
 * Navigate to a route, wait for it to fully load.
 */
export async function navigateAndWait(page: Page, route: string) {
	await page.goto(route, { waitUntil: 'networkidle' });
	// Give Svelte time to hydrate
	await page.waitForTimeout(500);
}

export const test = base;
