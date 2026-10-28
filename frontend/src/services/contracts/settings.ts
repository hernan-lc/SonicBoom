import type { Json, JsonObject } from './json';

export interface ConfigSnapshot {
  revision: number;
  config: JsonObject;
}
export type ApplyStrategy = 'hot' | 'restart_server' | 'restart_subsystem' | 'reload_model' | 'restart_process';
export interface SettingSchema {
  path: string;
  description: string;
  type: 'string' | 'integer' | 'number' | 'boolean' | 'array' | 'object';
  apply: ApplyStrategy;
  min?: number;
  max?: number;
  allowed_values?: string[];
  nullable: boolean;
  secret: boolean;
  allowed_values_source?: string;
}
export type Schema = Record<string, SettingSchema>;
export interface ConfigStatus {
  pending_restart?: boolean;
  valid?: boolean;
  revision: number;
  config_path: string;
  watcher?: string;
  last_reload?: string;
  last_error?: string;
  pending_applies?: string[];
  apply_failures?: { path: string; error: string; active: Json }[];
}
