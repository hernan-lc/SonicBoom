const storageKey = 'sonicboom.guides.done';

export function completedGuides(): string[] {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is string => typeof id === 'string');
  } catch {
    return [];
  }
}

export function isGuideDone(id: string): boolean {
  return completedGuides().includes(id);
}

export function markGuideDone(id: string): void {
  try {
    const ids = new Set(completedGuides());
    ids.add(id);
    localStorage.setItem(storageKey, JSON.stringify([...ids]));
  } catch {
    /* Guides stay usable when storage is unavailable. */
  }
}
