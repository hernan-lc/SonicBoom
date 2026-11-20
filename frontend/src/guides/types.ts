export interface GuideStep {
  target: `[data-guide="${string}"]`;
  titleKey: string;
  textKey: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  /** Selector of a control to press before showing this step. */
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
