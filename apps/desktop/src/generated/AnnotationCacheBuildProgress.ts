import type { AnnotationCacheBuildPhase } from "./AnnotationCacheBuildPhase.js";

export type AnnotationCacheBuildProgress = { files_done: number, files_total: number, tokens_done: bigint, tokens_total: bigint, current_phase: AnnotationCacheBuildPhase, };
