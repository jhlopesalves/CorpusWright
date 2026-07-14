import assert from "node:assert/strict";
import test from "node:test";

import { GenerationCounter } from "../src/app/generation.ts";
import { normaliseIpcError } from "../src/app/ipc-error.ts";

test("generation invalidation rejects an older completion", () => {
  const generations = new GenerationCounter<"preview">();
  const oldRequest = generations.next();
  const currentRequest = generations.next();
  assert.equal(generations.isCurrent(oldRequest), false);
  assert.equal(generations.isCurrent(currentRequest), true);
  generations.invalidate();
  assert.equal(generations.isCurrent(currentRequest), false);
});

test("IPC classification preserves visible string wording", () => {
  const message = "Corpus has been reloaded. Please try again.";
  assert.deepEqual(normaliseIpcError(message), {
    code: "stale_result",
    message,
    retryable: true,
  });
  assert.equal(normaliseIpcError("Analysis build cancelled.").code, "cancelled");
  assert.equal(normaliseIpcError("memory limit exceeded").code, "resource_limit");
});
