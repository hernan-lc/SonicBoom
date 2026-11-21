import AxeBuilder from '@axe-core/playwright';
import { test, expect } from './test';
import { studioFixtures } from './studioFixtures';

const popover = (page: import('@playwright/test').Page) => page.locator('.driver-popover');
// Driver's own popover is also a role=dialog, so exclude it.
const creator = (page: import('@playwright/test').Page) => page.locator('[role="dialog"]:not(.driver-popover)');

test('the pronunciation guide opens its collapsed panel, then closes cleanly', async ({ page }) => {
  await studioFixtures(page);
  await page.goto('/tests/e2e/index.html');
  const trigger = page.locator('[data-guide="pronunciation-toggle"]');
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  // Launched from the gallery, where the panel on Create is still collapsed.
  await page.getByRole('tab', { name: 'Guides', exact: true }).click();
  await page.getByRole('button', { name: /Pronunciation guide/ }).first().click();
  await expect(popover(page)).toBeVisible();
  await expect(popover(page)).toContainText('Pronunciation mode');
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('[data-guide="pronunciation-mode"]')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(popover(page)).toHaveCount(0);
});

test('the pronunciation guide does not close an already open panel', async ({ page }) => {
  await studioFixtures(page);
  await page.goto('/tests/e2e/index.html');
  const trigger = page.locator('[data-guide="pronunciation-toggle"]');
  await trigger.click();
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  await page.locator('.guide-hint[aria-label*="Pronunciation guide"]').click();
  await expect(popover(page)).toContainText('Pronunciation mode');
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
});

test('the clone guide shows the New voice button first and opens the dialog only on Next', async ({ page }) => {
  // No fade-in: axe would measure the popover's colours halfway through the animation.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await studioFixtures(page);
  await page.goto('/tests/e2e/index.html');
  await page.getByRole('tab', { name: 'Voice library', exact: true }).click();
  await page.locator('.guide-hint[aria-label*="Clone a voice"]').first().click();
  await expect(popover(page)).toContainText('New voice');
  await expect(creator(page)).toHaveCount(0);
  await popover(page).locator('.driver-popover-next-btn').click();
  await expect(creator(page)).toBeVisible();
  await expect(popover(page)).toContainText('Pick an engine');
  // Only the popover: the dimming overlay behind it makes a whole-page contrast audit meaningless.
  expect((await new AxeBuilder({ page }).include('.driver-popover').analyze()).violations).toEqual([]);
});
