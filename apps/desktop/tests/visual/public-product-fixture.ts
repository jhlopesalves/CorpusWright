/** Boots the unchanged application; only native IPC is replaced with controlled core responses. */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import "../../src/main";
import type { ProjectSource } from "../../src/generated/ProjectSource";
import type { FilePreview } from "../../src/generated/FilePreview";

const data = fetch("/public-product-data").then(response => response.json());
let builtOverview: unknown = null;
let pendingRequests = 0;
const failures: string[] = [];
window.addEventListener("error", event => failures.push(event.message));
window.addEventListener("unhandledrejection", event => failures.push(String(event.reason)));
mockWindows("main");
mockIPC(async (command, rawArgs = {}) => {
  const args = rawArgs as Record<string, unknown>;
  pendingRequests += 1;
  try {
    const payload = await data;
    const files = payload.scan.files;
    const sources: ProjectSource[] = files.map((file: { source_path: string; relative_path: string }, index: number) => ({
      id: `demo-${index}`, path: file.source_path, file_name: file.relative_path,
      extension: "txt", kind: "txt", status: "text_extracted", reason_codes: [],
      extraction_route: "plain_text", pdf_profile: null, ocr_quality: null,
      added_at: "2026-10-05T12:00:00Z", last_checked_at: null, last_processed_at: null,
    }));
    const count = sources.length;
    const summary = {
      total_count: count, loadable_count: count, extracted_text_count: count, processed_count: 0,
      warning_count: 0,
      status_counts: { unprocessed: 0, ready_to_process: 0, needs_ocr: 0, text_extracted: count,
        text_extracted_with_warnings: 0, processed: 0, processed_with_warnings: 0, failed: 0 },
      kind_counts: { pdf: 0, docx: 0, txt: count, html: 0, unknown: 0 },
    };
    switch (command) {
      case "set_window_titlebar_color": return null;
      case "plugin:dialog|open": return "Demonstration corpus";
      case "open_corpus_project_command": return {
        name: "Shared garden", rootPath: "Demonstration corpus", manifestPath: "Demonstration corpus/corpuswright.json",
        projectVersion: 1, sourceRoots: [], language: "en", description: null,
      };
      case "load_repeated_artifact_decisions_command": return '{"version":1,"decisions":{}}';
      case "query_project_sources_command": return {
        total_count: count, offset: 0, limit: 200, items: sources, summary, inventory_summary: summary,
      };
      case "load_project_source_records_command": return { summary: payload.scan.summary, corpusVersion: 1, recordCount: count };
      case "query_project_source_selection_action_summary_command": return {
        totalSelected: 1, allSelectedArePdfs: false, usablePdfCount: 0, extractablePdfCount: 0, reextractablePdfCount: 0, failedPdfCount: 0,
      };
      case "query_selected_source_preview_viewport_command": {
        const selection = args.selection as { ids: string[] };
        const selected = sources.filter(source => selection.ids.includes(source.id));
        const previews = (args.processed ? payload.processed : payload.original).files;
        return { total_selected: selected.length, viewport_start: 0, viewport_size: 4,
          items: selected.map(source => ({ source, preview: previews.find((file: FilePreview) => file.source_path === source.path), preview_error: null })) };
      }
      case "get_analysis_build_status_command": return { state: builtOverview ? "ready" : "idle", progress: null, error: null, overview: builtOverview };
      case "get_annotation_cache_availability_command": return { state: "missing", detail: null };
      case "compute_corpus_overview_command": if (builtOverview) return builtOverview;
    }
    if (command.startsWith("plugin:window|")) return false;
    const response = await fetch("/public-product-ipc", {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ command, args }),
    });
    const result = await response.json();
    if (!response.ok) { failures.push(result.error); throw new Error(result.error); }
    if (command === "build_analysis_corpus_command") builtOverview = result;
    return result;
  } finally {
    pendingRequests -= 1;
  }
}, { shouldMockEvents: true });

Object.assign(window, { publicProductFailures: failures });
Object.defineProperty(window, "publicProductPendingRequests", { get: () => pendingRequests });
Object.defineProperty(window, "publicProductOverview", { get: () => builtOverview });
window.addEventListener("DOMContentLoaded", () => document.querySelector<HTMLButtonElement>("#btn-open-project")!.click());
