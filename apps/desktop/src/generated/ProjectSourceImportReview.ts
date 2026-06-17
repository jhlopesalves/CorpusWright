import type { ProjectSource } from "./ProjectSource.js";

export type ProjectSourceImportReview = { candidates: Array<ProjectSource>, duplicate_count: number, ignored_count: number, };
