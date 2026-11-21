<script setup lang="ts">
import BaseInput from '../ui/BaseInput.vue';
import { t } from '../../i18n';
defineProps<{ disabled: boolean; errors: Record<string, string> }>();
const name = defineModel<string>('name', { required: true }), consent = defineModel<string>('consent', { required: true });
</script>
<template><fieldset :disabled="disabled"><legend class="sr-only">{{ t('Name and consent') }}</legend>
  <BaseInput id="cloneName" v-model="name" :label="t('Name')" :error="errors.cloneName" required autocomplete="off"/>
  <BaseInput id="cloneConsent" v-model="consent" :label="t('Consent basis')" :error="errors.cloneConsent" :placeholder="t('I own this voice or have permission')" required autocomplete="off"/>
</fieldset></template>
