<script setup lang="ts">
import { ref, watch } from 'vue';
import { t } from '../../i18n';
import BaseButton from '../ui/BaseButton.vue';
import CloneEngineStep from '../voice/CloneEngineStep.vue';
import CloneReferenceStep from '../voice/CloneReferenceStep.vue';
import CloneReviewStep from '../voice/CloneReviewStep.vue';
import FeedbackNotice from '../FeedbackNotice.vue';
import { useCloneWizard } from '../../features/cloning/useCloneWizard';
const emit = defineEmits<{ created: []; engines: [] }>();
const { originalFile, applyTrim, restoreOriginal, created, fieldErrors, cancelAnalysis, createAnother, tryVoice, limits, metadata, pending, feedback, provider, model, language, mode, referenceWarnings, referenceDefects, referenceGuidance, referenceMessage, name, consent, transcript, file, choices, providers, cap, languages, needsTranscript, engineReady, selectFile, save } = useCloneWizard(event => event === 'created' ? emit('created') : emit('engines'));
const wizard = ref<HTMLElement | null>(null);
watch(created, () => {
  wizard.value?.closest<HTMLElement>('.voice-creator-body')?.scrollTo?.({ top: 0 });
});
</script>
<template>
  <section ref="wizard" data-guide="clone-wizard" class="clone-wizard" aria-labelledby="clone-heading"><h2 id="clone-heading" class="sr-only">{{ t('Add a voice') }}</h2>
    <FeedbackNotice :feedback="feedback"/>
    <form v-if="!created" id="cloneForm" :aria-busy="pending" novalidate @submit.prevent="save()">
      <CloneReferenceStep v-model:mode="mode" v-model:transcript="transcript" :disabled="pending" :analyzing="pending" :provider="provider" :modes="cap?.clone_modes || []" :file="file" :original-file="originalFile" :metadata="metadata" :limits="limits" :errors="fieldErrors" :needs-transcript="needsTranscript" @select="selectFile" @trim="applyTrim" @restore="restoreOriginal"/>
      <CloneEngineStep v-model:provider="provider" v-model:model="model" v-model:language="language" :choices="choices" :providers="providers" :cap="cap" :languages="languages" :disabled="pending"/>
      <CloneReviewStep v-model:name="name" v-model:consent="consent" :disabled="pending" :errors="fieldErrors"/>
      <div class="toolbar"><button v-if="pending" type="button" @click="cancelAnalysis">{{ t('Cancel') }}</button><BaseButton id="cloneSubmit" data-guide="clone-submit" type="submit" variant="primary" :disabled="!cap" :loading="pending">{{ t(pending ? 'Cloning…' : 'Clone voice') }}</BaseButton><button v-if="!engineReady" type="button" class="btn-inline" @click="emit('engines')">{{ t('Manage engines') }}</button></div>
    </form>
    <section v-if="created" id="cloneResult" tabindex="-1" class="clone-report" aria-labelledby="clone-result-heading">
      <h3 id="clone-result-heading">{{ t('Voice created') }}: {{ created.name }}</h3>
      <p>{{ t('Voice profile ID') }}: <code>{{ created.id }}</code></p>
      <p v-for="defect in referenceDefects" :key="'defect-' + defect" role="alert" class="feedback error">{{ referenceMessage(defect) }}</p>
      <p v-for="warning in referenceWarnings" :key="'warn-' + warning" role="status" class="help-note">{{ referenceMessage(warning) }}</p>
      <p v-for="note in referenceGuidance" :key="'note-' + note" role="status" class="muted">{{ referenceMessage(note) }}</p>
      <div class="toolbar"><button type="button" class="primary" @click="tryVoice">{{ t('Try voice') }}</button><button type="button" @click="createAnother">{{ t('Create another') }}</button></div>
    </section>
  </section>
</template>
