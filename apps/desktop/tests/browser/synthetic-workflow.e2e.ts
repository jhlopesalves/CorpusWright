import { expect } from "@wdio/globals";

describe("runtime-generated synthetic corpus workflow", () => {
  it("completes project, preparation, analysis, recovery, export, and reopen", async () => {
    const commands = [
      "create_corpus_project_command",
      "load_files_command",
      "preview_files_command",
      "preview_processed_files_command",
      "build_analysis_corpus_command",
      "query_frequency_list_command",
      "query_concordance_collocations_command",
      "cancel_concordance_command",
      "export_corpus_command",
      "open_corpus_project_command",
    ];
    const mocks = new Map<string, Awaited<ReturnType<typeof browser.tauri.mock>>>();
    for (const command of commands) {
      const mock = await browser.tauri.mock(command);
      await mock.mockResolvedValue({ ok: true });
      mocks.set(command, mock);
    }
    const concordance = await browser.tauri.mock("search_concordance_command");
    await concordance.mockResolvedValueOnce({ rows: [], distribution: { items: [] } });
    await concordance.mockRejectedValueOnce("synthetic backend failure");
    await concordance.mockResolvedValue({ rows: [], distribution: { items: [] } });

    const run = await $("#browser-workflow-run");
    await run.waitForClickable();
    await run.click();
    const reopened = await $("[data-step='project-reopened']");
    await reopened.waitForExist({ timeout: 20_000 });

    const expected = [
      "project-created", "corpus-imported", "source-previewed", "processed-previewed",
      "cleaning-changed", "analysis-built", "frequency-queried", "concordance-queried",
      "collocates-queried", "operation-cancelled", "backend-error-recovered",
      "corpus-exported", "project-reopened",
    ];
    const actual = await $$("#browser-workflow-log [data-step]").map((item) =>
      item.getAttribute("data-step")
    );
    expect(actual).toEqual(expected);

    const imported = mocks.get("load_files_command")!;
    await imported.update();
    expect(imported).toHaveBeenCalledTimes(1);
    expect(imported.mock.calls[0]![0].paths).toHaveLength(3);
    expect(imported.mock.calls[0]![0].paths[0]).toContain("generated-1.txt");
  });
});
