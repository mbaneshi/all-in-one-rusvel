/**
 * Global setup for Playwright E2E tests.
 *
 * Seeds the API with a test session and goal so that pages have content to render.
 * Writes the session ID to a temp file that test fixtures read.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const API = 'http://localhost:3000';
const STATE_FILE = path.join(__dirname, '..', 'test-results', '.test-state.json');

export interface TestState {
	sessionId: string;
	sessionName: string;
}

async function globalSetup() {
	// Ensure output dir exists
	const dir = path.dirname(STATE_FILE);
	if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

	// Wait for API to be ready
	for (let i = 0; i < 30; i++) {
		try {
			const res = await fetch(`${API}/api/health`);
			if (res.ok) break;
		} catch {
			/* server not ready */
		}
		await new Promise((r) => setTimeout(r, 1000));
	}

	// Create a test session
	const sessionRes = await fetch(`${API}/api/sessions`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name: 'E2E Test Session', kind: 'General' })
	});
	if (!sessionRes.ok) throw new Error(`Failed to create session: ${sessionRes.status}`);
	const { id: sessionId } = (await sessionRes.json()) as { id: string };

	// Create a goal so the dashboard has content
	await fetch(`${API}/api/sessions/${sessionId}/mission/goals`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			title: 'Launch MVP',
			description: 'Ship the first version of RUSVEL',
			timeframe: 'Week'
		})
	});

	const state: TestState = { sessionId, sessionName: 'E2E Test Session' };
	fs.writeFileSync(STATE_FILE, JSON.stringify(state));
	console.log(`[global-setup] Created test session: ${sessionId}`);
}

export default globalSetup;
