import type { ProjectSourceStatusCounts } from "./ProjectSourceStatusCounts.js";
import type { ProjectSourceKindCounts } from "./ProjectSourceKindCounts.js";

export type ProjectSourceSummary = { total_count: number, loadable_count: number, extracted_text_count: number, processed_count: number, warning_count: number, status_counts: ProjectSourceStatusCounts, kind_counts: ProjectSourceKindCounts, };
