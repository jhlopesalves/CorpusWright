import type { ProjectLanguage } from "./ProjectLanguage.js";

export type ActiveProject = { name: string, rootPath: string, manifestPath: string, projectVersion: number, sourceRoots: Array<string>, language: ProjectLanguage | null, description: string | null, };
