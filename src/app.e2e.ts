import { expect, test } from '@playwright/test';

test('home shows Database Manager shell', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByText('Database Manager')).toBeVisible();
	await expect(page.getByText('Connections')).toBeVisible();
});
