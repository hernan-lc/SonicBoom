import { t } from '../i18n';
import { markGuideDone } from './progress';
import { guides } from './registry';
import type { Config, DriveStep, Driver } from 'driver.js';
import type { GuideStep } from './types';

type DriverModule = typeof import('driver.js');

export interface RunnerDeps {
  doc?: Pick<Document, 'querySelector'>;
  location?: Pick<Location, 'hash'>;
  matchMedia?: (query: string) => { matches: boolean };
  isVisible?: (element: Element) => boolean;
  loadDriver?: () => Promise<DriverModule>;
  sleep?: (ms: number) => Promise<void>;
  waitMs?: number;
  step?: number;
}

const pollMs = 50;

async function loadDriver(): Promise<DriverModule> {
  await import('driver.js/dist/driver.css');
  return import('driver.js');
}

function visible(element: Element): boolean {
  try {
    const style = getComputedStyle(element);
    return (
      style.display !== 'none' &&
      style.visibility !== 'hidden' &&
      style.opacity !== '0' &&
      element.getClientRects().length > 0
    );
  } catch {
    return false;
  }
}

function press(selector: string, doc: RunnerDeps['doc']): void {
  const element = doc?.querySelector(selector) as HTMLElement | null;
  if (typeof element?.click === 'function') element.click();
}

interface Ctx {
  doc: NonNullable<RunnerDeps['doc']>;
  isVisible: NonNullable<RunnerDeps['isVisible']>;
  sleep: NonNullable<RunnerDeps['sleep']>;
  waitMs: number;
}

/**
 * Wait for a step's target to be visible. If it has an opener (a dialog button, a collapsed
 * panel), press it once, as soon as the opener itself is on screen: after a route change the
 * page may not have rendered yet, and pressing twice would close what the first press opened.
 */
async function reveal(step: GuideStep, opener: string | undefined, ctx: Ctx): Promise<boolean> {
  const deadline = Date.now() + ctx.waitMs;
  let pressed = false;
  for (;;) {
    const target = ctx.doc.querySelector(step.target);
    if (target && ctx.isVisible(target)) return true;
    if (opener && !pressed) {
      const control = ctx.doc.querySelector(opener);
      if (control && ctx.isVisible(control)) {
        press(opener, ctx.doc);
        pressed = true;
      }
    }
    if (Date.now() >= deadline) return false;
    await ctx.sleep(pollMs);
  }
}

/** The control that must be pressed to reach `index`: its own opener or the closest earlier one. */
function openerFor(steps: GuideStep[], index: number): string | undefined {
  for (let i = index; i >= 0; i--) if (steps[i].opens) return steps[i].opens;
  return undefined;
}

export async function startGuide(id: string, deps: RunnerDeps = {}): Promise<boolean> {
  const guide = guides.find((item) => item.id === id);
  if (!guide) return false;

  const doc = deps.doc ?? document;
  const location = deps.location ?? globalThis.location;
  const isVisible = deps.isVisible ?? visible;
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  const waitMs = deps.waitMs ?? 3000;

  if (location.hash !== guide.route) location.hash = guide.route;

  const ctx: Ctx = { doc, isVisible, sleep, waitMs };
  const requested = Number.isInteger(deps.step) ? (deps.step as number) : 0;
  const start = Math.min(Math.max(requested, 0), Math.max(guide.steps.length - 1, 0));
  let first = -1;
  for (let index = start; index < guide.steps.length && first < 0; index++) {
    if (await reveal(guide.steps[index], openerFor(guide.steps, index), ctx)) first = index;
  }
  if (first < 0) return false;

  let module: DriverModule;
  try {
    module = await (deps.loadDriver ?? loadDriver)();
  } catch {
    return false;
  }

  const shown = guide.steps.slice(first);
  const steps: DriveStep[] = shown.map((step, index) => {
    const next = shown[index + 1];
    return {
      element: step.target,
      popover: {
        title: t(step.titleKey),
        description: t(step.textKey),
        side: step.side,
        // A later step may live behind a control (a dialog, a collapsed panel): press it only
        // now, so the steps before it still point at what the reader is looking at.
        ...(next?.opens
          ? {
              onNextClick: async (_element, _step, { driver }) => {
                await reveal(next, next.opens, ctx);
                driver.moveNext();
              },
            }
          : {}),
      },
    };
  });

  const query = '(prefers-reduced-motion: reduce)';
  const reduceMotion = deps.matchMedia
    ? deps.matchMedia(query).matches
    : globalThis.matchMedia?.(query).matches === true;

  return new Promise<boolean>((resolve) => {
    let settled = false;
    const finish = (complete: boolean) => {
      if (settled) return;
      settled = true;
      resolve(complete);
    };
    const config: Config = {
      steps,
      animate: !reduceMotion,
      showProgress: true,
      progressText: t('{current} of {total}', { current: '{{current}}', total: '{{total}}' }),
      nextBtnText: t('Next'),
      prevBtnText: t('Previous'),
      doneBtnText: t('Done'),
      closeBtnLabel: t('Close'),
      skipMissingElement: true,
      onDestroyed: (_element, _step, options) => {
        const complete = options.reason === 'done';
        if (complete) markGuideDone(guide.id);
        finish(complete);
      },
    };
    try {
      const instance: Driver = module.driver(config);
      instance.drive();
    } catch {
      finish(false);
    }
  });
}
