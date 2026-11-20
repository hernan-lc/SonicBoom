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

function press(selector: string, doc: RunnerDeps['doc']): void {
  const element = doc?.querySelector(selector) as HTMLElement | null;
  if (typeof element?.click === 'function') element.click();
}

async function prepareSteps(
  guide: (typeof guides)[number],
  doc: RunnerDeps['doc'],
  isVisible: NonNullable<RunnerDeps['isVisible']>,
  sleep: NonNullable<RunnerDeps['sleep']>,
  waitMs: number,
): Promise<boolean[]> {
  const ready: boolean[] = [];
  for (const step of guide.steps) {
    if (step.opens) press(step.opens, doc);
    ready.push(
      step.opens ? await waitForTarget(step.target, doc, isVisible, sleep, waitMs) : !!doc?.querySelector(step.target),
    );
  }
  return ready;
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

  const ready = await prepareSteps(guide, doc, isVisible, sleep, waitMs);
  const requested = Number.isInteger(deps.step) ? deps.step! : 0;
  const start = Math.min(Math.max(requested, 0), Math.max(guide.steps.length - 1, 0));
  if (!ready.slice(start).some(Boolean)) return false;

  let module: DriverModule;
  try {
    module = await (deps.loadDriver ?? loadDriver)();
  } catch {
    return false;
  }

  const steps: DriveStep[] = guide.steps
    .map((step, index) => ({ step, index }))
    .filter(({ index }) => index >= start && ready[index])
    .map(({ step }) => ({
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
