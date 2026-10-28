import { afterEach, it, expect, vi } from 'vitest';
import { monitorJob } from '../../src/services/jobs';
import { ApiError } from '../../src/services/http';
afterEach(() => vi.useRealTimers());
const stream = (data: string) => new Response('data: ' + data + '\n\n', { headers: { 'Content-Type': 'text/event-stream' } });
it('reconciles the same job after disconnect without creating another job', async () => {
  vi.useFakeTimers();
  const api = { job: vi.fn().mockResolvedValueOnce({ id: 'one', stage: 'generating' }).mockResolvedValueOnce({ id: 'one', stage: 'completed' }), jobEvents: vi.fn(async () => { throw new TypeError('Disconnected'); }), createJob: vi.fn() };
  const pending = monitorJob(api as any, 'one', () => {}, new AbortController().signal);
  await vi.advanceTimersByTimeAsync(1001);
  expect((await pending).stage).toBe('completed');
  expect(api.job).toHaveBeenNthCalledWith(2, 'one', expect.any(AbortSignal));
  expect(api.createJob).not.toHaveBeenCalled();
});
it('validates SSE JSON and rejects foreign job snapshots without retrying', async () => {
  for (const [payload, kind] of [['{', 'invalid_json'], [JSON.stringify({ id: 'foreign', stage: 'completed' }), 'invalid_response'], [JSON.stringify({ id: 'one', stage: [] }), 'invalid_response']]) {
    const api = { job: vi.fn(async () => ({ id: 'one', stage: 'generating' })), jobEvents: vi.fn(async () => stream(payload)) };
    await expect(monitorJob(api as any, 'one', () => {}, new AbortController().signal)).rejects.toMatchObject({ kind });
    expect(api.job).toHaveBeenCalledTimes(1);
  }
});
it('carries long-form fragment progress through polling and SSE snapshots', async () => {
  const seen: number[][] = [];
  const events = [
    JSON.stringify({ id: 'one', stage: 'generating', fragments_total: 4, fragments_done: 1 }),
    JSON.stringify({ id: 'one', stage: 'completed', fragments_total: 4, fragments_done: 4 }),
  ].map(line => `data: ${line}\n\n`).join('');
  const api = {
    job: vi.fn(async () => ({ id: 'one', stage: 'generating', fragments_total: 4, fragments_done: 1 })),
    jobEvents: vi.fn(async () => new Response(events, { headers: { 'Content-Type': 'text/event-stream' } })),
  };
  const final = await monitorJob(api as any, 'one', job => { seen.push([job.fragments_done ?? -1, job.fragments_total ?? -1]); }, new AbortController().signal);
  expect(final.fragments_total).toBe(4);
  expect(final.fragments_done).toBe(4);
  // Polling snapshot first, then SSE progress: ordered, never duplicated or reordered.
  expect(seen[0]).toEqual([1, 4]);
  expect(seen[seen.length - 1]).toEqual([4, 4]);
});
it('does not loop on expired authentication or throttling and preserves Retry-After', async () => {
  for (const status of [401, 429]) {
    const api = { job: vi.fn(async () => { throw new ApiError(status, 'Rejected', { retryAfter: 30 }); }) };
    await expect(monitorJob(api as any, 'one', () => {}, new AbortController().signal)).rejects.toMatchObject({ status, retryAfter: 30 });
    expect(api.job).toHaveBeenCalledTimes(1);
  }
});
