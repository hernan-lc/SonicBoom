<script setup lang="ts">
import { DialogRoot, DialogPortal, DialogOverlay, DialogContent, DialogTitle, DialogDescription, DialogClose } from 'reka-ui';
import { t } from '../../i18n';
import AppIcon from '../ui/AppIcon.vue';
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
              <AppIcon name="close" />
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
