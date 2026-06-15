import type { ProjectSourceKind } from "./ProjectSourceKind.js";
import type { ProjectSourceStatus } from "./ProjectSourceStatus.js";

export type ProjectSource = { id: string, path: string, file_name: string, extension: string | null, kind: ProjectSourceKind, status: ProjectSourceStatus, reason_codes: Array<string>, added_at: string, last_checked_at: string | null, last_processed_at: string | null, };
