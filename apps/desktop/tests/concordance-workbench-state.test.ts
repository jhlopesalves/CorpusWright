import assert from "node:assert/strict";
import type { ConcordanceResult } from "../src/generated/ConcordanceResult.ts";

import {
  CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY,
  acceptConcordanceCardResponse,
  acceptConcordanceSectionResponse,
  applyConcordanceLinesContextFilter,
  beginConcordanceCardRequest,
  clearConcordanceLinesContextFilter,
  collapseConcordanceCard,
  concordanceCardRequestKey,
  createConcordanceWorkbenchState,
  expandConcordanceCard,
  invalidateConcordanceCard,
  planConcordanceLocalRequest,
  planConcordanceSharedRequest,
  readConcordanceCollapsedCards,
  rejectConcordanceCardResponse,
  updateConcordanceSharedState,
  writeConcordanceCollapsedCards,
  type StorageLike,
} from "../src/workbench/concordance/state.ts";

class MemoryStorage implements StorageLike {
  values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

{
  const state = createConcordanceWorkbenchState(shared());
  state.lines.window = 9;
  state.lines.sort = "right2";
  state.lines.pageSize = 250;
  state.lines.offset = 500;
  state.lines.collapsed = true;
  state.collocates.span = 3;
  state.collocates.direction = "left";
  const collocatesKey = concordanceCardRequestKey(state, "collocates");
  const sharedBefore = { ...state.shared };

  applyConcordanceLinesContextFilter(state, "pair", "right");
  assert.deepEqual(state.shared, sharedBefore);
  assert.equal(state.lines.window, 9);
  assert.equal(state.lines.sort, "right2");
  assert.equal(state.lines.pageSize, 250);
  assert.equal(state.lines.offset, 0);
  assert.equal(state.lines.collapsed, false);
  assert.equal(state.lines.contextFilter, "pair");
  assert.equal(state.lines.contextFilterDirection, "right");
  assert.equal(concordanceCardRequestKey(state, "collocates"), collocatesKey);
  assert.equal(state.collocates.span, 3);
  assert.equal(state.collocates.direction, "left");

  clearConcordanceLinesContextFilter(state);
  assert.equal(state.lines.contextFilter, null);
  assert.equal(state.lines.contextFilterDirection, "both");
  assert.equal(state.lines.window, 9);
  assert.equal(state.lines.sort, "right2");
  assert.equal(state.lines.pageSize, 250);
  assert.deepEqual(state.shared, sharedBefore);
  assert.equal(JSON.parse(concordanceCardRequestKey(state, "lines")).contextFilter, null);
}

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
    sortKey: "filePosition",
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
  const storage = new MemoryStorage();
  const state = createConcordanceWorkbenchState(shared(), new Set(["lines", "distribution"]));
  writeConcordanceCollapsedCards(state, storage);
  assert.deepEqual(JSON.parse(storage.values.get(CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY)!), [
    "distribution",
    "lines",
  ]);
  assert.deepEqual([...readConcordanceCollapsedCards(storage)].sort(), ["distribution", "lines"]);
  storage.values.set(CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY, "not-json");
  assert.deepEqual([...readConcordanceCollapsedCards(storage)], []);
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
  collapseConcordanceCard(state, "lines");
  collapseConcordanceCard(state, "collocates");
  assert.deepEqual(planConcordanceSharedRequest(state), {
    concordance: {
      includeRows: false,
      includeDistribution: true,
      cards: ["distribution"],
    },
    collocates: false,
  });
  collapseConcordanceCard(state, "distribution");
  assert.deepEqual(planConcordanceSharedRequest(state), {
    concordance: null,
    collocates: false,
  });
  assert.equal(beginConcordanceCardRequest(state, "lines"), null);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const request = beginConcordanceCardRequest(state, "lines")!;
  assert(acceptConcordanceCardResponse(state, "lines", request.generation, request.requestKey, {
    rows: ["first"],
  }));
  assert.equal(expandConcordanceCard(state, "lines"), "cache");
  state.lines.offset = 100;
  assert.equal(expandConcordanceCard(state, "lines"), "query");
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
  const request = beginConcordanceCardRequest(state, "collocates")!;
  collapseConcordanceCard(state, "collocates");
  assert.equal(state.collocates.loading, false);
  assert.equal(
    acceptConcordanceCardResponse(state, "collocates", request.generation, request.requestKey, {}),
    false,
  );
  assert.equal(state.collocates.data, null);
}

{
  const state = createConcordanceWorkbenchState(shared());
  collapseConcordanceCard(state, "distribution");
  const changed = updateConcordanceSharedState(state, shared("changed"));
  assert.deepEqual(changed, ["lines", "collocates", "distribution"]);
  assert.equal(state.lines.loading, false);
  assert.equal(state.collocates.loading, false);
  assert.equal(state.distribution.loading, false);
  assert.equal(planConcordanceSharedRequest(state).concordance?.includeDistribution, false);
}

{
  const state = createConcordanceWorkbenchState(shared());
  collapseConcordanceCard(state, "lines");
  assert.equal(invalidateConcordanceCard(state, "lines"), false);
  assert.equal(invalidateConcordanceCard(state, "distribution"), true);

  const distributionKey = concordanceCardRequestKey(state, "distribution");
  state.lines.window = 9;
  assert.equal(concordanceCardRequestKey(state, "distribution"), distributionKey);
  state.collocates.span = 2;
  assert.equal(concordanceCardRequestKey(state, "distribution"), distributionKey);

  assert.deepEqual(planConcordanceLocalRequest(state, "lines"), []);
  assert.deepEqual(planConcordanceLocalRequest(state, "distribution"), ["distribution"]);
  assert.deepEqual(planConcordanceLocalRequest(state, "collocates"), ["collocates"]);
}

{
  const state = createConcordanceWorkbenchState(shared());
  assert.deepEqual(planConcordanceLocalRequest(state, "lines"), ["lines"]);
  assert.deepEqual(planConcordanceLocalRequest(state, "distribution"), ["distribution"]);
  assert.deepEqual(planConcordanceLocalRequest(state, "collocates"), ["collocates"]);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const lines = beginConcordanceCardRequest(state, "lines")!;
  const accepted = acceptConcordanceSectionResponse(
    state,
    new Map([["lines", lines]]),
    sectionResult([], null),
  );
  assert.deepEqual(accepted, ["lines"]);
  assert.deepEqual((state.lines.data as ConcordanceResult).rows, []);
  assert.equal(state.distribution.data, null);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const distribution = beginConcordanceCardRequest(state, "distribution")!;
  const page = { totalDocuments: 2, offset: 0, limit: 10, items: [], maxCount: 0 };
  const accepted = acceptConcordanceSectionResponse(
    state,
    new Map([["distribution", distribution]]),
    sectionResult(null, page),
  );
  assert.deepEqual(accepted, ["distribution"]);
  assert.deepEqual((state.distribution.data as ConcordanceResult).distribution, page);
  assert.equal(state.lines.data, null);
}

{
  const state = createConcordanceWorkbenchState(shared());
  const lines = beginConcordanceCardRequest(state, "lines")!;
  const distribution = beginConcordanceCardRequest(state, "distribution")!;
  const page = { totalDocuments: 2, offset: 0, limit: 10, items: [], maxCount: 0 };
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
