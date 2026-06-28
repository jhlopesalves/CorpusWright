import type { AnnotationCacheBuildPhase } from "./AnnotationCacheBuildPhase.js";

export type AnnotationCacheBuildProgress = { files_done: number, files_total: number, current_phase: AnnotationCacheBuildPhase, };
