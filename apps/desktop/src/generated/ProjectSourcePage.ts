import type { ProjectSource } from "./ProjectSource.js";
import type { ProjectSourceSummary } from "./ProjectSourceSummary.js";

export type ProjectSourcePage = { total_count: number, offset: number, limit: number, items: Array<ProjectSource>, summary: ProjectSourceSummary, inventory_summary: ProjectSourceSummary, };
