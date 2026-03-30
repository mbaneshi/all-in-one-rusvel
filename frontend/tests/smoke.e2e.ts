/**
 * Behavioral smoke E2E — no screenshot baselines (see *.visual.ts).
 * Relies on global-setup session + fixtures localStorage injection.
 */

import { expect } from '@playwright/test';
import { test, setupSession, navigateAndWait } from './fixtures';

test.describe('Smoke E2E', () => {
	test.beforeEach(async ({ page }) => {
		await setupSession(page);
	});

	test('dashboard has main landmark and nav', async ({ page }) => {
		await navigateAndWait(page, '/');
		await expect(page.locator('main')).toBeVisible();
		await expect(page.getByRole('link', { name: 'Dashboard' })).toBeVisible();
	});

	test('settings page shows Settings heading', async ({ page }) => {
		await navigateAndWait(page, '/settings');
		await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
	});

	test('department forge route loads', async ({ page }) => {
		await navigateAndWait(page, '/dept/forge');
		await expect(page.locator('main')).toBeVisible();
		const bodyText = await page.locator('body').textContent();
		expect(bodyText).not.toMatch(/500|Internal Server Error/i);
	});
});
