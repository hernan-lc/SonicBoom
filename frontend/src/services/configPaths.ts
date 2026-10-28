import type { Json, JsonObject } from './contracts';

/** Read a dotted path out of the config JSON tree. */
export function getPath(object: JsonObject | undefined, path: string): Json | undefined {
  return path
    .split('.')
    .reduce<Json | undefined>(
      (value, key) => (value && typeof value === 'object' && !Array.isArray(value) ? value[key] : undefined),
      object,
    );
}

/** Wrap a value in nested objects for a dotted config path. */
export function nestedChange(path: string, value: Json): JsonObject {
  return path
    .split('.')
    .reverse()
    .reduce<Json>((result, key) => ({ [key]: result }), value) as JsonObject;
}
