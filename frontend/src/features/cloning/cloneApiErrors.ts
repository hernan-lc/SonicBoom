import { t, msg, computedMessage, renderMessage, type Message } from '../../i18n';
import type { ApiError } from '../../services/http';

const fieldIds: Record<string, string> = {
  name: 'cloneName',
  transcript: 'cloneTranscript',
  consent_basis: 'cloneConsent',
  provider: 'cloneProvider',
  model: 'cloneModel',
  language: 'cloneLang',
  mode: 'cloneMode',
  file: 'cloneFile',
};

const identityFields = ['cloneName', 'cloneConsent'];
const engineFields = ['cloneProvider', 'cloneModel', 'cloneLang'];

/** Wizard step that owns a given field. */
export function stepForField(field: string): number {
  if (identityFields.includes(field)) return 2;
  if (engineFields.includes(field)) return 0;
  return 1;
}

/**
 * Turn a server field-error response into form issues. The correlation id
 * travels with each field message so it stays attached to the visible error.
 */
export function cloneFieldIssues(error: ApiError, nameMaxChars: number) {
  const trace = error.requestId ? ` (${t('Request ID: {id}', { id: error.requestId })})` : '';
  const issues: Record<string, Message> = {};
  for (const issue of error.fields ?? []) {
    const id = fieldIds[issue.field];
    if (!id) continue;
    const base =
      issue.field === 'transcript' && issue.code === 'required'
        ? msg('ICL mode needs the exact reference transcript.')
        : issue.field === 'transcript' && issue.code === 'too_long' && typeof issue.expected === 'number'
          ? msg('Transcript exceeds {max} characters.', { max: issue.expected })
          : issue.field === 'name'
            ? msg('Name must be 1–{max} characters.', { max: nameMaxChars })
            : issue.field === 'consent_basis'
              ? msg('Consent basis is required (e.g. owner).')
              : msg(issue.message);
    issues[id] = trace ? computedMessage(() => renderMessage(base) + trace) : base;
  }
  return issues;
}
