export interface GuideStep {
  target: `[data-guide="${string}"]`;
  titleKey: string;
  textKey: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  /** Control to press, only if the target is not visible yet, when moving to this step (opens a dialog or a collapsed panel). */
  opens?: string;
}

export interface Guide {
  id: string;
  route: string;
  titleKey: string;
  summaryKey: string;
  seconds: number;
  steps: GuideStep[];
}
