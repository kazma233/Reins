// Latest-wins guard for fire-and-forget request sequences: each request
// captures the current token, and only the holder of the latest token may
// apply its result. Invalidate() bumps the token so in-flight results are all
// dropped without waiting for them to settle.
export type RequestGuard = {
  next: () => number;
  isLatest: (id: number) => boolean;
  invalidate: () => void;
};

export function createRequestGuard(): RequestGuard {
  let current = 0;

  return {
    next: () => ++current,
    isLatest: (id) => current === id,
    invalidate: () => {
      ++current;
    },
  };
}

// Key-based variant for guards whose invalidation signal is an external value
// change (selected session, target app) rather than a monotonically increasing
// token. `read` should access the reactive source so both capture-time and
// compare-time reads are current.
export type KeyGuard<T> = {
  capture: () => T;
  isCurrent: (key: T) => boolean;
};

export function createKeyGuard<T>(read: () => T): KeyGuard<T> {
  return {
    capture: read,
    isCurrent: (key) => read() === key,
  };
}
