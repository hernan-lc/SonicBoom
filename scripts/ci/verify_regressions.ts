#!/usr/bin/env node
/**
 * Verify that every security-regression test still exists exactly once.
 *
 * The authoritative behavioral check is the main `cargo test --all-targets` run.
 * libtest, however, exits successfully when a `-- <filter>` matches nothing, so a
 * renamed or deleted regression test can silently vanish. This script consumes a
 * single `cargo test --all-targets -- --list` listing and fails when any required
 * test is missing or ambiguous (matched more than once).
 *
 * Usage:
 *     cargo test --locked --all-targets -- --list > listing.txt
 *     node --experimental-strip-types scripts/ci/verify_regressions.ts listing.txt
 */
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

// Keep this list synchronized with real test function names. Each entry is the
// leaf (unqualified) name; the module path is intentionally not pinned so
// moving a test between modules does not break the guard.
export const REQUIRED: readonly string[] = [
  'cancellation_does_not_release_running_permit',
  'cancellation_with_pending_queue_keeps_running_slot',
  'spoofed_leftmost_entry_cannot_be_selected',
  'multiple_trusted_proxies_skipped_right_to_left',
  'malformed_chain_fails_safe',
  'multiple_xff_headers_do_not_allow_spoofing',
  'enqueue_rejects_beyond_bound_without_dropping',
  'dead_audio_thread_reports_unavailable_on_every_operation',
  'stop_clears_current_and_queued_temps',
  'sweep_deletes_only_owned_uuid_wavs',
  'failed_create_does_not_touch_live_memory',
  'failed_revoke_leaves_token_active_in_memory',
  'malformed_integers_fail_instead_of_defaulting',
  'malformed_booleans_fail_instead_of_defaulting',
  'bounded_values_are_enforced',
  'content_length_mismatch_is_rejected',
  'oversized_stream_is_aborted_and_cleaned',
  'download_timeout_cleans_temporary_file',
  'unknown_models_are_rejected',
  'unknown_voices_are_rejected_not_defaulted',
  'unknown_formats_are_rejected_not_defaulted',
  'malformed_expiry_is_rejected_not_perpetual',
  'hostile_voice_names_are_escaped',
  'volume_accepts_only_finite_unit_range',
];

/** Count leaf test names in a `cargo test -- --list` listing.
 *
 * Only `name: test` and `name: bench` lines are counted; doctests and
 * `--list` headers are ignored. The leaf is the final `::`-separated segment.
 */
export function parseListing(text: string): Map<string, number> {
  const counts = new Map<string, number>();
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!(line.endsWith(': test') || line.endsWith(': bench'))) continue;
    const qualified = line.slice(0, line.lastIndexOf(':')).trim();
    if (!qualified) continue;
    const leaf = qualified.split('::').at(-1)!;
    counts.set(leaf, (counts.get(leaf) ?? 0) + 1);
  }
  return counts;
}

export function check(text: string, required: readonly string[] = REQUIRED): number {
  const counts = parseListing(text);
  const missing = required.filter((name) => (counts.get(name) ?? 0) === 0);
  const duplicates = required.filter((name) => (counts.get(name) ?? 0) > 1);
  const problems: string[] = [];
  if (missing.length) problems.push('missing regression tests: ' + missing.join(', '));
  if (duplicates.length) {
    const detail = duplicates.map((name) => `${name} (x${counts.get(name)})`).join(', ');
    problems.push('ambiguous regression tests matched more than once: ' + detail);
  }
  if (problems.length) throw new Error(problems.join('; '));
  return required.length;
}

function main(argv: string[]): number {
  if (argv.length !== 3) {
    console.error(`Usage: node --experimental-strip-types scripts/ci/verify_regressions.ts <listing>`);
    return 2;
  }
  const total = check(readFileSync(argv[2], 'utf8'));
  console.log(`All ${total} security regression tests are present exactly once`);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = main(process.argv);
}
