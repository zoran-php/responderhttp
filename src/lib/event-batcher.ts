// http_client/src/lib/event-batcher.ts
//
// Coalesces a stream of events into batches, so a WebSocket receiving 1,000
// messages a second costs one store update per frame rather than 1,000
// (PLAN.md Phase 13c). Order is always preserved: a batch is exactly the
// events pushed since the last one, in push order.

/** Runs `flush` once, later — in the app, on the next animation frame. */
export type FlushScheduler = (flush: () => void) => void;

export interface EventBatcherOptions<T> {
  /**
   * Events that must not wait for a frame. The batch they end goes out at
   * once, with everything queued before them.
   */
  isUrgent: (event: T) => boolean;
  /**
   * Flush at this many queued events without waiting. Animation frames stop
   * while the window is hidden in the tray, and a connection keeps
   * receiving; this keeps the queue bounded.
   */
  maxBatch: number;
}

export interface EventBatcher<T> {
  push: (event: T) => void;
  /**
   * Delivers whatever is queued, now. A WebSocket runs until it is closed
   * and never needs this; a request ends, and its last blocks must not
   * arrive after the caller has already seen the finished response.
   */
  flush: () => void;
}

export function createEventBatcher<T>(
  onFlush: (batch: T[]) => void,
  schedule: FlushScheduler,
  { isUrgent, maxBatch }: EventBatcherOptions<T>,
): EventBatcher<T> {
  let queued: T[] = [];
  let scheduled = false;

  const flush = (): void => {
    scheduled = false;
    if (queued.length === 0) {
      return;
    }
    const batch = queued;
    queued = [];
    onFlush(batch);
  };

  return {
    flush,
    push(event) {
      queued.push(event);
      if (isUrgent(event) || queued.length >= maxBatch) {
        // A frame may still be pending; it finds the queue empty, or holding
        // only what arrived after this flush, which is what it is for.
        const batch = queued;
        queued = [];
        onFlush(batch);
        return;
      }
      if (!scheduled) {
        scheduled = true;
        schedule(flush);
      }
    },
  };
}
