import { computed, ref, watch, onBeforeUnmount, reactive, nextTick } from 'vue';
import { uiError, msg, renderMessage, errorMessage, type Message } from '../../i18n';
import { useStudio } from '../../composables/useStudio';
import { ApiError } from '../../services/http';
import { useTask } from '../../composables/useTask';
import { validateReference, type ReferenceMetadata } from './validateReference';
import { referenceMessage } from './referenceMessages';
import { useCloneSelection } from './useCloneSelection';
import { cloneFieldIssues, stepForField } from './cloneApiErrors';
import type { VoiceCreateResult, VoiceConstraints } from '../../services/contracts';

export function useCloneWizard(emit: (event: 'created' | 'engines') => void) {
  const fieldIssues = reactive<Record<string, Message>>({});
  const fieldErrors = computed(() =>
    Object.fromEntries(Object.entries(fieldIssues).map(([field, issue]) => [field, renderMessage(issue)])),
  );
  const created = ref<VoiceCreateResult | null>(null);
  let analysis: AbortController | null = null;
  let serial = 0;
  let revalidate = false;
  async function invalid(field: string, message: Message): Promise<never> {
    fieldIssues[field] = message;
    fieldFailure = true;
    step.value = ['cloneName', 'cloneConsent'].includes(field) ? 2 : field === 'cloneProvider' ? 0 : 1;
    await nextTick();
    document.getElementById(field)?.focus();
    throw Object.assign(new Error(renderMessage(message)), { localizedMessage: message });
  }
  function cancelAnalysis() {
    ++serial;
    analysis?.abort();
  }
  const limits = ref<VoiceConstraints | null>(null);
  const metadata = ref<ReferenceMetadata | null>(null);
  let disposed = false;
  onBeforeUnmount(() => {
    disposed = true;
    cancelAnalysis();
  });
  const studio = useStudio();
  const { capabilities, providers } = studio;
  const identity = () => studio.scopeEpoch?.value ?? studio.token?.value;
  const task = useTask();
  const { pending, feedback } = task;
  let fieldFailure = false;
  // A failure attached to a field is shown next to that field, so the summary banner stays out of the way.
  async function runWizard(action: () => Promise<unknown>) {
    fieldFailure = false;
    await task.run(action);
    if (fieldFailure) feedback.value = null;
  }
  watch(pending, async (value) => {
    if (!value && revalidate && file.value && !created.value) {
      revalidate = false;
      void runWizard(() => validateRecording(false));
      return;
    }
    if (!value && Object.keys(fieldIssues).length) {
      await nextTick();
      document.getElementById(Object.keys(fieldIssues)[0])?.focus();
    }
  });
  const { step, provider, model, language, mode, choices, cap, languages, needsTranscript, engineReady } =
    useCloneSelection(capabilities, providers, limits);
  const referenceWarnings = ref<string[]>([]);
  const referenceDefects = ref<string[]>([]);
  const referenceGuidance = ref<string[]>([]);
  const originalFile = ref<File | null>(null);
  const name = ref('');
  const consent = ref('');
  const transcript = ref('');
  const file = ref<File | null>(null);
  watch(name, () => {
    delete fieldIssues.cloneName;
  });
  watch(consent, () => {
    delete fieldIssues.cloneConsent;
  });
  watch(transcript, () => {
    delete fieldIssues.cloneTranscript;
  });
  watch(needsTranscript, (value) => {
    if (!value) transcript.value = '';
  });
  async function selectFile(selected: File | null, trimmed = false) {
    if (!trimmed) originalFile.value = selected;
    cancelAnalysis();
    metadata.value = null;
    referenceWarnings.value = [];
    referenceDefects.value = [];
    referenceGuidance.value = [];
    file.value = selected;
    delete fieldIssues.cloneFile;
    if (selected) await runWizard(() => validateRecording(false));
  }
  async function validateRecording(checkTranscript = true) {
    for (const key of Object.keys(fieldIssues)) delete fieldIssues[key];
    const current = ++serial;
    analysis = new AbortController();
    if (!file.value) return invalid('cloneFile', msg('Choose a WAV reference recording.'));
    if (!file.value.size) throw uiError('The selected file is empty.');
    limits.value = await studio.api.voiceConstraints(analysis.signal);
    const selected = file.value;
    let result: ReferenceMetadata;
    try {
      result = await validateReference(selected, limits.value, provider.value, analysis.signal);
    } catch (error) {
      if (analysis.signal.aborted || current !== serial) throw uiError('Request cancelled.');
      return invalid('cloneFile', errorMessage(error));
    }
    if (disposed || current !== serial || file.value !== selected)
      throw uiError('Reference selection changed. Please try again.');
    metadata.value = result;
    const constraint = limits.value.providers[provider.value];
    if (
      !constraint?.models.includes(model.value) ||
      !constraint.clone_modes.includes(mode.value) ||
      !languages.value.includes(language.value)
    )
      return invalid('cloneProvider', msg('Engine selection changed. Choose a supported model, mode and language.'));
    if ([...transcript.value.trim()].length > limits.value.transcript_max_chars)
      return invalid(
        'cloneTranscript',
        msg('Transcript exceeds {max} characters.', { max: limits.value.transcript_max_chars }),
      );
    if (checkTranscript && needsTranscript.value && !transcript.value.trim())
      return invalid('cloneTranscript', msg('ICL mode needs the exact reference transcript.'));
  }
  async function next() {
    await runWizard(async () => {
      if (step.value === 0) {
        if (!cap.value) throw uiError('No cloning engine is available right now.');
        if (!model.value) throw uiError('Pick a model.');
      } else {
        await validateRecording();
      }
      step.value++;
    });
  }
  async function save() {
    const owner = identity();
    await runWizard(async () => {
      await validateRecording();
      if (disposed || owner !== identity()) return;
      if (!name.value.trim() || [...name.value.trim()].length > limits.value!.name_max_chars)
        return invalid('cloneName', msg('Name must be 1–{max} characters.', { max: limits.value!.name_max_chars }));
      if (!consent.value.trim()) return invalid('cloneConsent', msg('Consent basis is required (e.g. owner).'));
      referenceWarnings.value = [];
      referenceDefects.value = [];
      referenceGuidance.value = [];
      let profile: VoiceCreateResult;
      try {
        profile = await studio.api.createVoice(
          {
            expected_revision: limits.value?.revision,
            name: name.value.trim(),
            provider: provider.value,
            model: model.value,
            language: language.value,
            mode: mode.value,
            consent_basis: consent.value.trim(),
            ...(needsTranscript.value ? { transcript: transcript.value.trim() } : {}),
          },
          file.value!,
        );
      } catch (error) {
        if (disposed || owner !== identity()) return;
        if (error instanceof ApiError && error.fields) {
          Object.assign(fieldIssues, cloneFieldIssues(error, limits.value!.name_max_chars));
          const id = Object.keys(fieldIssues)[0];
          if (id) {
            fieldFailure = true;
            step.value = stepForField(id);
          }
        }
        throw error;
      }
      if (disposed || owner !== identity()) return;
      referenceWarnings.value = profile.warnings || [];
      referenceDefects.value = profile.defects || [];
      referenceGuidance.value = profile.guidance || [];
      created.value = profile;
      await studio.refreshVoices();
      if (disposed || owner !== identity()) return;
      studio.selectProfile(profile);
      feedback.value = {
        kind: 'success',
        message: msg('Voice saved. Preview it before using it in a longer recording.'),
      };
      await nextTick();
      document.getElementById('cloneResult')?.focus();
    });
    if (owner !== identity()) feedback.value = null;
  }
  watch(
    identity,
    () => {
      cancelAnalysis();
      created.value = null;
      metadata.value = null;
      referenceWarnings.value = [];
      referenceDefects.value = [];
      referenceGuidance.value = [];
    },
    { flush: 'sync' },
  );
  async function applyTrim(selected: File) {
    await selectFile(selected, true);
  }
  async function restoreOriginal() {
    if (originalFile.value) await selectFile(originalFile.value, true);
  }
  function createAnother() {
    created.value = null;
    originalFile.value = null;
    step.value = 0;
    name.value = '';
    consent.value = '';
    transcript.value = '';
    file.value = null;
    metadata.value = null;
  }
  function tryVoice() {
    if (created.value) {
      studio.selectProfile(created.value, true);
      emit('created');
    }
  }
  watch([provider, model, mode, language], () => {
    // The recording does not change, so its measurements stay visible while the new selection is checked.
    cancelAnalysis();
    if (!file.value || created.value) return;
    if (pending.value) revalidate = true;
    else {
      revalidate = false;
      void runWizard(() => validateRecording(false));
    }
  });
  return {
    originalFile,
    applyTrim,
    restoreOriginal,
    created,
    fieldErrors,
    cancelAnalysis,
    createAnother,
    tryVoice,
    limits,
    metadata,
    pending,
    feedback,
    step,
    provider,
    model,
    language,
    mode,
    referenceWarnings,
    referenceDefects,
    referenceGuidance,
    referenceMessage,
    name,
    consent,
    transcript,
    file,
    choices,
    cap,
    languages,
    needsTranscript,
    engineReady,
    selectFile,
    next,
    save,
  };
}
