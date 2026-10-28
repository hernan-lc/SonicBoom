/** Imperative handle exposed by WaveformPlayer for cross-player coordination. */
export type PlayerHandle = {
  play: () => Promise<unknown> | undefined;
  pause: () => void;
};
