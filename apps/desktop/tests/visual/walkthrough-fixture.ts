/** Run the desktop UI unchanged with the walkthrough's scratch corpus behind native IPC. */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import "../../src/main";
import type { DocumentRecord } from "../../src/generated/DocumentRecord.js";
import type { FilePreview } from "../../src/generated/FilePreview.js";
import type { ProjectSource } from "../../src/generated/ProjectSource.js";

const corpusName = "Walkthrough corpus";
const projectName = "Probability, logic and calculation";
const failures: string[] = [];
let pendingRequests = 0;
let builtOverview: unknown = null;
let decisions = '{"version":1,"decisions":{}}';
let choosingExport = false;

async function callCore(command: string, args: Record<string, unknown>): Promise<any> {
  const response = await fetch("/walkthrough-ipc", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args }),
  });
  const packet = await response.json();
  if (typeof packet.error === "string") throw new Error(packet.error);
  return packet.value;
}

function projectSource(record: DocumentRecord, index: number): ProjectSource {
  const kind = record.document_type === "text" ? "txt" : record.document_type;
  const routes = { pdf: "embedded_text", html: "html_extract", txt: "plain_text", docx: "docx_extract" } as const;
  return {
    id: `walkthrough-${index}`,
    path: record.source_path,
    file_name: record.relative_path.split("/").at(-1) ?? record.relative_path,
    extension: record.relative_path.split(".").at(-1) ?? kind,
    kind,
    status: "text_extracted",
    reason_codes: [],
    extraction_route: routes[kind],
    pdf_profile: null,
    ocr_quality: null,
    added_at: "2026-11-21T12:00:00Z",
    last_checked_at: null,
    last_processed_at: null,
  };
}

const boot = callCore("bootstrap", {}).then(scan => {
  const sources: ProjectSource[] = scan.files.map(projectSource);
  const count = sources.length;
  const kindCounts = { pdf: 0, docx: 0, txt: 0, html: 0, unknown: 0 };
  sources.forEach(source => { kindCounts[source.kind] += 1; });
  const summary = {
    total_count: count,
    loadable_count: count,
    extracted_text_count: count,
    processed_count: 0,
    warning_count: 0,
    status_counts: { unprocessed: 0, ready_to_process: 0, needs_ocr: 0, text_extracted: count,
      text_extracted_with_warnings: 0, processed: 0, processed_with_warnings: 0, failed: 0 },
    kind_counts: kindCounts,
  };
  return { scan, sources, count, summary };
});

window.addEventListener("error", event => failures.push(event.message));
window.addEventListener("unhandledrejection", event => failures.push(String(event.reason)));
mockWindows("main");
mockIPC(async (command, rawArgs = {}) => {
  const args = rawArgs as Record<string, unknown>;
  pendingRequests += 1;
  try {
    const { scan, sources, count, summary } = await boot;
    switch (command) {
      case "cancel_repeated_artifacts_command":
      case "set_window_titlebar_color":
        return null;
      case "plugin:dialog|open":
        return choosingExport ? "Walkthrough export" : corpusName;
      case "open_corpus_project_command":
        return { name: projectName, rootPath: corpusName, manifestPath: `${corpusName}/corpuswright.json`,
          projectVersion: 1, sourceRoots: [], language: "en", description: null };
      case "load_repeated_artifact_decisions_command":
        return decisions;
      case "save_repeated_artifact_decisions_command":
        decisions = String(args.content);
        return null;
      case "query_project_sources_command":
        return { total_count: count, offset: 0, limit: 200, items: sources, summary, inventory_summary: summary };
      case "load_project_source_records_command":
        return { summary: scan.summary, corpusVersion: 1, recordCount: count };
      case "query_project_source_selection_action_summary_command":
        return { totalSelected: 1, allSelectedArePdfs: false, usablePdfCount: 0, extractablePdfCount: 0,
          reextractablePdfCount: 0, failedPdfCount: 0 };
      case "query_selected_source_preview_viewport_command": {
        const selection = args.selection as { ids: string[] };
        const selected = sources.filter(source => selection.ids.includes(source.id));
        const start = Number(args.viewportStart);
        const size = Number(args.viewportSize);
        const visible = selected.slice(start, start + size);
        const previews: FilePreview[] = await callCore("preview_selected", {
          paths: visible.map(source => source.path),
          processed: args.processed,
          cleaningConfig: args.cleaningConfig,
        });
        return { total_selected: selected.length, viewport_start: start, viewport_size: size,
          items: visible.map(source => ({
            source, preview: previews.find(preview => preview.source_path === source.path), preview_error: null,
          })) };
      }
      case "get_analysis_build_status_command":
        return { state: builtOverview ? "ready" : "idle", progress: null, error: null, overview: builtOverview };
      case "get_annotation_cache_availability_command":
        return { state: "missing", detail: null };
      case "compute_corpus_overview_command":
        return builtOverview;
    }
    if (command.startsWith("plugin:window|")) return false;
    const result = await callCore(command, args);
    if (command === "build_analysis_corpus_command") builtOverview = result;
    if (command === "export_corpus_command") {
      Object.assign(window, { walkthroughExportExcerpt: result.exportExcerpt });
    }
    return result;
  } catch (error) {
    failures.push(`${command}: ${error instanceof Error ? error.message : String(error)}`);
    throw error;
  } finally {
    pendingRequests -= 1;
  }
}, { shouldMockEvents: true });

Object.assign(window, { walkthroughFailures: failures, walkthroughChooseExport: () => { choosingExport = true; } });
Object.defineProperty(window, "walkthroughPendingRequests", { get: () => pendingRequests });
window.addEventListener("DOMContentLoaded", () => document.querySelector<HTMLButtonElement>("#btn-open-project")!.click());
