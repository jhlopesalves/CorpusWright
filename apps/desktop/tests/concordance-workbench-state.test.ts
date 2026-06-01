import assert from "node:assert/strict";
import type { ConcordanceResult } from "../src/generated/ConcordanceResult.ts";

import {
  acceptConcordanceCardResponse,
  acceptConcordanceSectionResponse,
  applyConcordanceLinesContextFilter,
  beginConcordanceCardRequest,
  clearConcordanceLinesContextFilter,
  concordanceCardRequestKey,
  createConcordanceWorkbenchState,
  executeConcordanceSharedRequest,
  invalidateConcordanceCard,
  planConcordanceLocalRequest,
  planConcordanceSharedRequest,
  rejectConcordanceCardResponse,
  updateConcordanceSharedState,
} from "../src/features/analyse/workbench/concordance/state.ts";

function shared(query = "target") {
  return {
    query,
    caseSensitive: false,
    fileScopeKey: "all-files",
    corpusVersion: 7,
    cleaningConfigKey: "cleaning-v1",
  };
}

function sectionResult(
  rows: ConcordanceResult["rows"],
  distribution: ConcordanceResult["distribution"],
): ConcordanceResult {
  return {
    query: "target",
    contextFilter: null,
    caseSensitive: false,
    sortKeys: ["filePosition"],
    windowTokens: 5,
    totalHits: 0,
    documentsWithHits: 0,
    selectedDocumentsTotal: 2,
    selectedTokensTotal: 20,
    dp: 0,
    dispersionLabel: "not available",
    topDocumentShare: 0,
    offset: 0,
    limit: 100,
    rows,
    distribution,
  };
}

{
  const state = createConcordanceWorkbenchState(shared());
  assert.equal(state.lines.window, 10);
  assert.equal(state.lines.pageSize, 24);
  state.lines.window = 9;
  state.lines.sort = "right2";
  state.lines.pageSize = 250;
  state.lines.offset = 500;
  state.collocates.span = 3;
  state.collocates.direction = "left";
  const collocatesKey = concordanceCardRequestKey(state, "collocates");
  const sharedBefore = { ...state.shared };

  applyConcordanceLinesContextFilter(state, "pair", "right");
  assert.deepEqual(state.shared, sharedBefore);
  assert.equal(state.lines.offset, 0);
  assert.equal(state.lines.contextFilter, "pair");
  assert.equal(state.lines.contextFilterDirection, "right");
  assert.equal(concordanceCardRequestKey(state, "collocates"), collocatesKey);

  clearConcordanceLinesContextFilter(state);
  assert.equal(state.lines.contextFilter, null);
  assert.equal(state.lines.contextFilterDirection, "both");
  assert.equal(state.lines.window, 9);
  assert.equal(state.lines.sort, "right2");
  assert.equal(state.lines.pageSize, 250);
}

{
  const state = createConcordanceWorkbenchState(shared("shared query"));
  const order: string[] = [];
  await executeConcordanceSharedRequest(
    planConcordanceSharedRequest(state),
    async () => {
      order.push("lines/distribution:start");
      await Promise.resolve();
      order.push("lines/distribution:settled");
      return true;
    },
    async () => {
      order.push("collocates:start");
    },
  );
  assert.deepEqual(order, [
    "lines/distribution:start",
    "lines/distribution:settled",
    "collocates:start",
  ]);

  order.length = 0;
  await executeConcordanceSharedRequest(
    planConcordanceSharedRequest(state),
    async () => {
      order.push("lines/distribution:failed");
      return false;
    },
    async () => {
      order.push("collocates:must-not-start");
    },
  );
  assert.deepEqual(order, ["lines/distribution:failed"]);
}

{
  const state = createConcordanceWorkbenchState(shared());
  assert.deepEqual(planConcordanceSharedRequest(state), {
    concordance: {
      includeRows: true,
      includeDistribution: true,
      cards: ["lines", "distribution"],
    },
    collocates: true,
  });
  assert.deepEqual(planConcordanceLocalRequest(state, "lines"), ["lines"]);
  assert.deepEqual(planConcordanceLocalRequest(state, "distribution"), ["distribution"]);
  assert.deepEqual(planConcordanceLocalRequest(state, "collocates"), []);
  assert.equal(invalidateConcordanceCard(state, "lines"), true);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const stale = beginConcordanceCardRequest(state, "distribution")!;
  const fresh = beginConcordanceCardRequest(state, "distribution")!;
  assert.equal(
    acceptConcordanceCardResponse(state, "distribution", stale.generation, stale.requestKey, {}),
    false,
  );
  assert(acceptConcordanceCardResponse(
    state,
    "distribution",
    fresh.generation,
    fresh.requestKey,
    { distribution: [] },
  ));
  assert.equal(
    rejectConcordanceCardResponse(state, "distribution", stale.generation, stale.requestKey, "old"),
    false,
  );
}

{
  const state = createConcordanceWorkbenchState(shared());
  const lines = { rows: ["preserved"] };
  state.lines.data = lines;
  const request = beginConcordanceCardRequest(state, "collocates")!;
  assert(rejectConcordanceCardResponse(
    state,
    "collocates",
    request.generation,
    request.requestKey,
    "Narrow the file scope or use a less frequent node, then retry.",
  ));
  assert.equal(state.lines.data, lines);
  assert.equal(state.lines.error, null);
  assert.equal(state.collocates.data, null);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const changed = updateConcordanceSharedState(state, shared("changed"));
  assert.deepEqual(changed, ["lines", "collocates", "distribution"]);
  assert.equal(state.lines.loading, false);
  assert.equal(state.collocates.loading, false);
  assert.equal(state.distribution.loading, false);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const lines = beginConcordanceCardRequest(state, "lines")!;
  const distribution = beginConcordanceCardRequest(state, "distribution")!;
  const page = { totalDocuments: 2, offset: 0, limit: 10, items: [], maxRatePer10k: 0 };
  const accepted = acceptConcordanceSectionResponse(
    state,
    new Map([
      ["lines", lines],
      ["distribution", distribution],
    ]),
    sectionResult([], page),
  );
  assert.deepEqual(accepted, ["lines", "distribution"]);
  assert.equal(state.lines.data, state.distribution.data);
}
