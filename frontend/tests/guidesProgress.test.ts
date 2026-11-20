import { test } from 'node:test';
import assert from 'node:assert/strict';
import { completedGuides, isGuideDone, markGuideDone } from '../src/guides/progress.ts';

const key = 'sonicboom.guides.done';
const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');

type Storage = Pick<StorageLike, 'getItem' | 'setItem'>;
interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function useStorage(storage: Storage) {
  Object.defineProperty(globalThis, 'localStorage', { value: storage, configurable: true, writable: true });
}
function restoreStorage() {
  if (original) Object.defineProperty(globalThis, 'localStorage', original);
  else Reflect.deleteProperty(globalThis, 'localStorage');
}
function memoryStorage() {
  const store = new Map<string, string>();
  return {
    getItem: (name: string) => store.get(name) ?? null,
    setItem: (name: string, value: string) => void store.set(name, String(value)),
    store,
  };
}

test('progress starts empty and remembers completed guides without duplicates', () => {
  const storage = memoryStorage();
  useStorage(storage);
  try {
    assert.deepEqual(completedGuides(), []);
    assert.equal(isGuideDone('first-audio'), false);
    markGuideDone('first-audio');
    markGuideDone('first-audio');
    markGuideDone('key-settings');
    assert.deepEqual(completedGuides(), ['first-audio', 'key-settings']);
    assert.equal(isGuideDone('first-audio'), true);
    assert.equal(JSON.parse(storage.store.get(key) ?? '[]').length, 2);
  } finally {
    restoreStorage();
  }
});

test('corrupt, empty and non-list storage reads as no progress', () => {
  for (const raw of ['', 'not json', '{"done":true}', 'null', '[1,"a",null]']) {
    useStorage({ getItem: () => raw, setItem: () => {} });
    try {
      assert.deepEqual(completedGuides(), raw === '[1,"a",null]' ? ['a'] : []);
    } finally {
      restoreStorage();
    }
  }
});

test('a storage that throws on reads and writes never breaks the guides', () => {
  useStorage({
    getItem: () => {
      throw new Error('blocked');
    },
    setItem: () => {
      throw new Error('blocked');
    },
  });
  try {
    assert.deepEqual(completedGuides(), []);
    assert.equal(isGuideDone('tokens-api'), false);
    assert.doesNotThrow(() => markGuideDone('tokens-api'));
  } finally {
    restoreStorage();
  }
});

test('missing storage is treated as session-only progress', () => {
  Reflect.deleteProperty(globalThis, 'localStorage');
  try {
    assert.deepEqual(completedGuides(), []);
    assert.doesNotThrow(() => markGuideDone('first-audio'));
  } finally {
    restoreStorage();
  }
});
