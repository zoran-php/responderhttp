// http_client/src/lib/capped-list.ts
//
// A list that keeps the newest entries and drops the oldest once it is over
// its caps, counting what it dropped. The WebSocket log and the event-stream
// viewer both need exactly this (PLAN-SSE.md, 14d); it was the WebSocket
// log's `appendCapped` before the second caller arrived.

export interface CappedCaps {
  maxEntries: number;
  maxBytes: number;
}

export interface CappedList<T> {
  /** Oldest first. Views that read newest first reverse as they render. */
  entries: T[];
  /** What `entries` holds, counted against `maxBytes`. */
  retainedBytes: number;
  /** How many the caps have evicted, for the "N earlier dropped" note. */
  droppedCount: number;
}

export function emptyCappedList<T>(): CappedList<T> {
  return { entries: [], retainedBytes: 0, droppedCount: 0 };
}

/**
 * Appends, then evicts the oldest entries until both caps hold. The newest
 * entry is always kept, even alone over the byte cap: a view that cannot
 * show what just arrived would be worse than one briefly over budget.
 */
export function appendCapped<T>(
  list: CappedList<T>,
  added: readonly T[],
  sizeOf: (entry: T) => number,
  caps: CappedCaps,
): CappedList<T> {
  if (added.length === 0) {
    return list;
  }
  const entries = list.entries.concat(added);
  let retainedBytes = list.retainedBytes + added.reduce((sum, entry) => sum + sizeOf(entry), 0);
  let evicted = 0;
  while (
    entries.length - evicted > 1 &&
    (entries.length - evicted > caps.maxEntries || retainedBytes > caps.maxBytes)
  ) {
    const oldest = entries[evicted];
    if (oldest === undefined) {
      break;
    }
    retainedBytes -= sizeOf(oldest);
    evicted += 1;
  }
  return {
    entries: evicted === 0 ? entries : entries.slice(evicted),
    retainedBytes,
    droppedCount: list.droppedCount + evicted,
  };
}
