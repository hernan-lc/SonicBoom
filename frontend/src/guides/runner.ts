import { t } from '../i18n';
import { markGuideDone } from './progress';
import { guides } from './registry';
import type { Config, DriveStep, Driver } from 'driver.js';

type DriverModule = typeof import('driver.js');

export interface RunnerDeps {
  doc?: Pick<Document, 'querySelector'>;
  location?: Pick<Location, 'hash'>;
  matchMedia?: (query: string) => { matches: boolean };
  isVisible?: (element: Element) => boolean;
  loadDriver?: () => Promise<DriverModule>;
  sleep?: (ms: number) => Promise<void>;
  waitMs?: number;
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

async function waitForTarget(
  target: string,
  doc: RunnerDeps['doc'],
  isVisible: NonNullable<RunnerDeps['isVisible']>,
  sleep: NonNullable<RunnerDeps['sleep']>,
  waitMs: number,
): Promise<boolean> {
  const deadline = Date.now() + waitMs;
  for (;;) {
    const element = doc?.querySelector(target);
    if (element && isVisible(element)) return true;
    if (Date.now() >= deadline) return false;
    await sleep(pollMs);
  }
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

  const first = guide.steps[0]?.target;
  if (first && !(await waitForTarget(first, doc, isVisible, sleep, waitMs))) return false;

  let module: DriverModule;
  try {
    module = await (deps.loadDriver ?? loadDriver)();
  } catch {
    return false;
  }

  const steps: DriveStep[] = guide.steps
    .filter((step) => doc.querySelector(step.target))
    .map((step) => ({
      element: step.target,
      popover: {
        title: t(step.titleKey),
        description: t(step.textKey),
        side: step.side,
      },
    }));
  if (!steps.length) return false;

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
