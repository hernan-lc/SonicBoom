<script setup lang="ts">
import { t } from '../../i18n';
import type { SamplingState } from '../../composables/useStudio';
import type { SynthesisConstraints } from '../../services/contracts';
import NumberField from '../ui/NumberField.vue';
defineProps<{
  sampling: SamplingState;
  issues: Record<string, string>;
  bounds?: SynthesisConstraints['sampling'];
  pending: boolean;
}>();
const emit = defineEmits<{ reset: [] }>();
</script>
<template>
  <details id="advancedGeneration">
    <summary>{{ t('Advanced generation settings') }}</summary>
    <p class="help-note">
      {{ t('Sampling presets, a fixed seed and quality detection. Defaults reproduce the stock behavior; changes are experimental.') }}
    </p>
    <div class="row">
      <div class="field">
        <label for="samplingPreset">{{ t('Preset') }}</label>
        <select id="samplingPreset" v-model="sampling.preset" :disabled="pending">
          <option value="balanced">{{ t('balanced') }}</option>
          <option value="stable">{{ t('stable') }}</option>
          <option value="expressive">{{ t('expressive') }}</option>
          <option value="custom">{{ t('custom') }}</option>
        </select>
      </div>
      <div class="field">
        <label for="samplingSeed">{{ t('Seed (optional)') }}</label>
        <input
          id="samplingSeed"
          v-model="sampling.seed"
          maxlength="20"
          :aria-invalid="!!issues.seed || undefined"
          aria-describedby="seed-error"
          inputmode="numeric"
          autocomplete="off"
          :placeholder="t('random')"
          :disabled="pending"
        />
      </div>
      <div class="field">
        <label for="qualityMode">{{ t('Quality detection') }}</label>
        <select id="qualityMode" v-model="sampling.quality" :disabled="pending">
          <option value="">{{ t('Server default') }}</option>
          <option value="off">{{ t('off') }}</option>
          <option value="report">{{ t('report') }}</option>
          <option value="retry">{{ t('retry') }}</option>
        </select>
      </div>
    </div>
    <fieldset :disabled="pending">
      <legend>{{ t('Talker sampling') }}</legend>
      <div class="row">
        <NumberField
          id="talkerTemperature"
          v-model="sampling.talkerTemperature"
          :label="t('Temperature')"
          :bounds="bounds?.temperature"
          :error="issues.talkerTemperature ? t(issues.talkerTemperature) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="talkerTopK"
          v-model="sampling.talkerTopK"
          :label="t('Top K')"
          :bounds="bounds?.top_k"
          :error="issues.talkerTopK ? t(issues.talkerTopK) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="talkerTopP"
          v-model="sampling.talkerTopP"
          :label="t('Top P')"
          :bounds="bounds?.top_p"
          :error="issues.talkerTopP ? t(issues.talkerTopP) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="talkerRepetitionPenalty"
          v-model="sampling.talkerRepetitionPenalty"
          :label="t('Repetition penalty')"
          :bounds="bounds?.repetition_penalty"
          :error="issues.talkerRepetitionPenalty ? t(issues.talkerRepetitionPenalty) : undefined"
          :disabled="pending"
        />
      </div>
    </fieldset>
    <p v-if="issues.seed" id="seed-error" class="field-error" role="alert">{{ t(issues.seed) }}</p>
    <details id="predictorSampling">
      <summary>{{ t('Predictor sampling (advanced)') }}</summary>
      <div class="row">
        <NumberField
          id="predictorTemperature"
          v-model="sampling.predictorTemperature"
          :label="t('Temperature')"
          :bounds="bounds?.temperature"
          :error="issues.predictorTemperature ? t(issues.predictorTemperature) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="predictorTopK"
          v-model="sampling.predictorTopK"
          :label="t('Top K')"
          :bounds="bounds?.top_k"
          :error="issues.predictorTopK ? t(issues.predictorTopK) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="predictorTopP"
          v-model="sampling.predictorTopP"
          :label="t('Top P')"
          :bounds="bounds?.top_p"
          :error="issues.predictorTopP ? t(issues.predictorTopP) : undefined"
          :disabled="pending"
        />
        <NumberField
          id="predictorRepetitionPenalty"
          v-model="sampling.predictorRepetitionPenalty"
          :label="t('Repetition penalty')"
          :bounds="bounds?.repetition_penalty"
          :error="issues.predictorRepetitionPenalty ? t(issues.predictorRepetitionPenalty) : undefined"
          :disabled="pending"
        />
      </div>
    </details>
    <div class="toolbar">
      <button id="resetSampling" type="button" :disabled="pending" @click="emit('reset')">{{ t('Reset to defaults') }}</button>
    </div>
  </details>
</template>
