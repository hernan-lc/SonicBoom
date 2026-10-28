<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { t } from "../../i18n";
import { useStudio } from "../../composables/useStudio";
import { useSynthesis } from "../../composables/useSynthesis";
import EmptyState from "../ui/EmptyState.vue";
import WaveformPlayer from "../audio/WaveformPlayer.vue";
import type { PlayerHandle } from "../audio/playerTypes";
import ModelSelector from "./ModelSelector.vue";
import VoiceSelector from "./VoiceSelector.vue";
import PronunciationSettings from "./PronunciationSettings.vue";
import SpokenPreview from "./SpokenPreview.vue";
import AdvancedSampling from "./AdvancedSampling.vue";
import GenerationStatus from "./GenerationStatus.vue";
import ResultComparison from "./ResultComparison.vue";
const emit = defineEmits<{ navigate: [tab: "voices" | "engines"] }>();
const studio = useStudio();
const {
    text,
    model,
    voice,
    language,
    models,
    selectedModel,
    availableVoices,
    preview,
    sampling,
    samplingIssues,
    info,
    resetSampling,
} = studio;
const {
    pending,
    cancelling,
    jobId,
    active,
    snapshot,
    results,
    audioUrl,
    audioFilename,
    audioBlob,
    error,
    complete,
    canGenerate,
    generate,
    regenerate,
    removeResult,
    resume,
    cancel,
} = useSynthesis(studio);
const player = ref<PlayerHandle | null>(null);
const comparisons = new Map<string, PlayerHandle>();
function registerComparison(id: string, handle: PlayerHandle | null) {
    if (handle) comparisons.set(id, handle);
    else comparisons.delete(id);
}
// One recording at a time: starting a preview silences the other versions.
function pauseComparisons(except?: string) {
    for (const [id, handle] of comparisons) if (id !== except) handle.pause();
}
function comparisonPlay(id: string) {
    player.value?.pause();
    pauseComparisons(id);
}
watch(pending, (value) => {
    if (value) pauseComparisons();
});
const languages = computed(() => selectedModel.value.languages);
// Advanced controls only exist for the Qwen3-TTS cloning engine.
const advancedAvailable = computed(
    () =>
        selectedModel.value.cloned &&
        selectedModel.value.provider === "qwen3-gguf",
);
const constraints = computed(() => info.value?.synthesis);
const samplingBounds = computed(() => constraints.value?.sampling);
const codepoints = computed(() => [...text.value].length);
const textBytes = computed(() => new TextEncoder().encode(text.value).length);
// Show byte counts only when they matter: near the server limit.
const bytesRelevant = computed(() => {
    if (!constraints.value) return textBytes.value > 0;
    return (
        textBytes.value / constraints.value.max_body_bytes > 0.5 ||
        codepoints.value / constraints.value.max_text_chars > 0.5
    );
});
// Remember the last voice/model/language across sessions.
const choiceKey = "sonicboom.createChoices";
try {
    const saved = localStorage.getItem(choiceKey);
    if (saved) {
        const parsed = JSON.parse(saved) as {
            model?: string;
            voice?: string;
            language?: string;
        };
        if (
            parsed.model &&
            models.value.some((item) => item.id === parsed.model)
        )
            model.value = parsed.model;
        if (
            parsed.voice &&
            availableVoices.value.some((item) => item.id === parsed.voice)
        )
            voice.value = parsed.voice;
        if (
            parsed.language &&
            selectedModel.value.languages.includes(parsed.language)
        )
            language.value = parsed.language;
    }
} catch {
    /* session-only */
}
watch([model, voice, language], ([nextModel, nextVoice, nextLanguage]) => {
    try {
        localStorage.setItem(
            choiceKey,
            JSON.stringify({
                model: nextModel,
                voice: nextVoice,
                language: nextLanguage,
            }),
        );
    } catch {
        /* session-only */
    }
});
const quality = computed(() => snapshot.value?.quality);
const qualityIssues = computed(() => quality.value?.issues ?? []);
watch(preview, () => void generate(true));
function detachPlayback() {
    player.value?.pause();
}
watch(
    audioUrl,
    (_value, previous) => {
        if (previous) detachPlayback();
    },
    { flush: "sync" },
);
onBeforeUnmount(() => {
    detachPlayback();
    pauseComparisons();
});
</script>
<template>
    <div class="studio-create">
        <form
            class="create-editor"
            @submit.prevent="generate()"
            :aria-busy="pending"
        >
            <div class="form-group">
                <div class="label-row">
                    <label for="text">{{
                        t("What would you like to say?")
                    }}</label
                    ><span id="charCount" class="char-count"
                        >{{
                            t(
                                codepoints === 1
                                    ? "{count} character"
                                    : "{count} characters",
                                { count: codepoints },
                            )
                        }}<template v-if="constraints">
                            / {{ constraints.max_text_chars }}</template
                        ><template v-if="bytesRelevant && constraints">
                            · {{ textBytes }} /
                            {{ constraints.max_body_bytes }} bytes</template
                        ></span
                    >
                </div>
                <textarea
                    id="text"
                    v-model="text"
                    :placeholder="
                        t(
                            'A story, an idea, a welcome message… Start writing here.',
                        )
                    "
                    :readonly="pending"
                />
            </div>
            <SpokenPreview />
            <div class="row">
                <div class="field">
                    <ModelSelector
                        v-model="model"
                        :models="models"
                        :disabled="pending"
                        @enable="emit('navigate', 'engines')"
                    />
                </div>
                <div class="field">
                    <label for="lang">{{ t("Language") }}</label
                    ><select id="lang" v-model="language" :disabled="pending">
                        <option
                            v-for="code in languages"
                            :key="code"
                            :value="code"
                        >
                            {{ t(`language.${code}`) }}
                        </option>
                    </select>
                </div>
            </div>
            <PronunciationSettings :disabled="pending" />
            <EmptyState
                v-if="selectedModel.cloned && !availableVoices.length"
                :title="t('No voices for this model yet.')"
                :hint="t('Create a voice to use this cloning model.')"
            >
                <button type="button" class="primary" @click="emit('navigate', 'voices')">
                    {{ t("Create a voice") }}
                </button>
            </EmptyState>
            <VoiceSelector
                v-else
                v-model="voice"
                :voices="availableVoices"
                :disabled="pending"
            />
            <AdvancedSampling
                v-if="advancedAvailable"
                :sampling="sampling"
                :issues="samplingIssues"
                :bounds="samplingBounds"
                :pending="pending"
                @reset="resetSampling"
            />
            <p v-if="!selectedModel.ready" class="help-note">
                {{ t("This model is not ready. Check its status in Engines.") }}
            </p>
            <p v-if="active" class="help-note">
                {{
                    t("Previous generation: {text}", {
                        text: active.input.text,
                    })
                }}
                · {{ active.input.model }} · {{ active.input.voice }} ·
                {{ active.input.language }}
            </p>
            <div class="toolbar">
                <button
                    id="btn"
                    class="primary"
                    :disabled="
                        pending ||
                        !!jobId ||
                        !canGenerate ||
                        (advancedAvailable &&
                            Object.keys(samplingIssues).length > 0)
                    "
                >
                    {{ t(pending ? "Generating…" : "Generate speech") }}</button
                ><button
                    v-if="results.length && !pending"
                    id="regenerate"
                    type="button"
                    @click="regenerate"
                >
                    {{ t("Regenerate") }}</button
                ><button
                    v-if="jobId"
                    type="button"
                    :disabled="pending"
                    @click="resume"
                >
                    {{ t("Resume previous generation") }}</button
                ><button
                    v-if="jobId"
                    id="cancelGeneration"
                    type="button"
                    :disabled="cancelling"
                    @click="cancel"
                >
                    {{
                        t(
                            cancelling
                                ? "Cancellation requested. Waiting for native computation to stop…"
                                : "Cancel generation",
                        )
                    }}
                </button>
            </div>
        </form>
        <aside class="create-side" :aria-label="t('Result')">
            <GenerationStatus
                :pending="pending"
                :snapshot="snapshot"
                :complete="complete"
                :quality="quality"
                :quality-issues="qualityIssues"
                :error="error"
                :job-id="jobId"
            />
            <template v-if="audioUrl"
                ><WaveformPlayer
                    id="player"
                    ref="player"
                    :blob="audioBlob"
                    :label="t('Latest generated audio')"
                    autoplay
                    @play="pauseComparisons()"
                /><a
                    id="downloadAudio"
                    :href="audioUrl"
                    :download="audioFilename"
                    >{{ t("Download generated audio") }}</a
                ></template
            >
            <p v-else class="output-hint">
                {{
                    t(
                        "Your audio will appear here, ready to listen or download.",
                    )
                }}
            </p>
            <ResultComparison
                :results="results"
                :register="registerComparison"
                @play="comparisonPlay"
                @remove="removeResult"
            />
        </aside>
    </div>
</template>
