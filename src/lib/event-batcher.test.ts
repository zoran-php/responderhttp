import { describe, expect, it } from "vitest";

import { createEventBatcher } from "@/lib/event-batcher";

/** A scheduler the test fires by hand, standing in for requestAnimationFrame. */
function manualFrames() {
  const pending: (() => void)[] = [];
  return {
    schedule: (flush: () => void) => {
      pending.push(flush);
    },
    get pendingCount() {
      return pending.length;
    },
    runFrame: () => {
      for (const flush of pending.splice(0)) {
        flush();
      }
    },
  };
}

function setup(maxBatch = 100) {
  const frames = manualFrames();
  const batches: string[][] = [];
  const batcher = createEventBatcher<string>((batch) => batches.push(batch), frames.schedule, {
    isUrgent: (event) => event.startsWith("!"),
    maxBatch,
  });
  return { frames, batches, batcher };
}

describe("createEventBatcher", () => {
  it("holds events until the frame and delivers them as one batch in order", () => {
    const { frames, batches, batcher } = setup();

    batcher.push("a");
    batcher.push("b");
    batcher.push("c");

    expect(batches).toEqual([]);
    expect(frames.pendingCount).toBe(1);
    frames.runFrame();
    expect(batches).toEqual([["a", "b", "c"]]);
  });

  it("flushes an urgent event at once, together with what was queued before it", () => {
    const { frames, batches, batcher } = setup();

    batcher.push("a");
    batcher.push("!closed");

    expect(batches).toEqual([["a", "!closed"]]);
    frames.runFrame();
    expect(batches).toEqual([["a", "!closed"]]);
  });

  it("keeps events after an urgent flush for the pending frame", () => {
    const { frames, batches, batcher } = setup();

    batcher.push("a");
    batcher.push("!connected");
    batcher.push("b");
    frames.runFrame();

    expect(batches).toEqual([["a", "!connected"], ["b"]]);
  });

  it("flushes at the size cap when frames are not running", () => {
    const { batches, batcher } = setup(3);

    for (const event of ["a", "b", "c", "d"]) {
      batcher.push(event);
    }

    expect(batches).toEqual([["a", "b", "c"]]);
  });

  it("schedules a new frame after one has run", () => {
    const { frames, batches, batcher } = setup();

    batcher.push("a");
    frames.runFrame();
    batcher.push("b");

    expect(frames.pendingCount).toBe(1);
    frames.runFrame();
    expect(batches).toEqual([["a"], ["b"]]);
  });
});
