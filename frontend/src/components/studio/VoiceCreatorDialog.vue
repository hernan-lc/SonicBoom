<script setup lang="ts">
import { DialogRoot, DialogPortal, DialogOverlay, DialogContent, DialogTitle, DialogDescription, DialogClose } from 'reka-ui';
import { t } from '../../i18n';
import CloneWizard from './CloneWizard.vue';
const open = defineModel<boolean>({ default: false });
defineEmits<{ created: []; engines: [] }>();
</script>
<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay class="dialog-overlay" />
      <DialogContent class="dialog-content voice-creator-dialog" @interact-outside="$event.preventDefault()">
        <header class="voice-creator-head">
          <div class="dialog-head">
            <div>
              <DialogTitle>{{ t('New voice') }}</DialogTitle>
              <DialogDescription>{{ t('Clone a voice in three steps: engine, reference, review.') }}</DialogDescription>
            </div>
            <DialogClose class="dialog-x" :aria-label="t('Close')" :title="t('Close')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
                <path d="m6 6 12 12M18 6 6 18" />
              </svg>
            </DialogClose>
          </div>
        </header>
        <div class="voice-creator-body">
          <CloneWizard @created="$emit('created')" @engines="$emit('engines')" />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
