import { invoke } from "../app/system-ipc";

/** Development-only renderer harness; the production build eliminates its import branch. */
export function installBrowserWorkflowHarness(): void {
  const panel = document.createElement("section");
  panel.id = "browser-workflow-harness";
  panel.innerHTML = `
    <h2>Browser workflow</h2>
    <button id="browser-workflow-run" type="button">Run synthetic workflow</button>
    <ol id="browser-workflow-log" aria-live="polite"></ol>
  `;
  document.body.append(panel);
  panel.querySelector<HTMLButtonElement>("#browser-workflow-run")!.addEventListener("click", () => {
    void runWorkflow(panel.querySelector<HTMLOListElement>("#browser-workflow-log")!);
  });
}

async function runWorkflow(log: HTMLOListElement): Promise<void> {
  const step = async (label: string, operation: () => Promise<unknown>) => {
    await operation();
    const item = document.createElement("li");
    item.dataset.step = label;
    item.textContent = label;
    log.append(item);
  };
  const paths = Array.from({ length: 3 }, (_, index) =>
    `/tmp/corpuswright-browser/generated-${index + 1}.txt`
  );
  const cleaningConfig = { browserSynthetic: true };

  await step("project-created", () => invoke("create_corpus_project_command", {
    request: {
      rootPath: "/tmp/corpuswright-browser/project",
      name: "Synthetic browser corpus",
      language: "en",
      description: null,
    },
  }));
  await step("corpus-imported", () => invoke("load_files_command", { paths }));
  await step("source-previewed", () => invoke("preview_files_command", {
    indices: [0], corpusVersion: 1, options: {},
  }));
  await step("processed-previewed", () => invoke("preview_processed_files_command", {
    indices: [0], corpusVersion: 1, cleaningConfig, options: {},
  }));
  await step("cleaning-changed", async () => {
    localStorage.setItem("corpuswright.browser.cleaning", JSON.stringify(cleaningConfig));
  });
  await step("analysis-built", () => invoke("build_analysis_corpus_command", {
    request: {
      indices: [0, 1, 2], corpusVersion: 1, buildSignature: "browser-v1", cleaningConfig,
    },
  }));
  await step("frequency-queried", () => invoke("query_frequency_list_command", {
    query: { offset: 0, limit: 25 }, cleaningConfig,
  }));
  await step("concordance-queried", () => invoke("search_concordance_command", {
    query: { query: "synthetic", includeRows: true, includeDistribution: true },
    corpusVersion: 1,
    cleaningConfig,
  }));
  await step("collocates-queried", () => invoke("query_concordance_collocations_command", {
    query: { query: "synthetic", limit: 25 }, corpusVersion: 1, cleaningConfig,
  }));
  await step("operation-cancelled", () => invoke("cancel_concordance_command"));
  try {
    await invoke("search_concordance_command", {
      query: { query: "recover" }, corpusVersion: 1, cleaningConfig,
    });
  } catch {
    await step("backend-error-recovered", () => invoke("search_concordance_command", {
      query: { query: "recover" }, corpusVersion: 1, cleaningConfig,
    }));
  }
  await step("corpus-exported", () => invoke("export_corpus_command", {
    options: { outputDir: "/tmp/corpuswright-browser/export" },
  }));
  localStorage.setItem("corpuswright.browser.project", "/tmp/corpuswright-browser/project");
  await step("project-reopened", () => invoke("open_corpus_project_command", {
    path: localStorage.getItem("corpuswright.browser.project"),
  }));
}
