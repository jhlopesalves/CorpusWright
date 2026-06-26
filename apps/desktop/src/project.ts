import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import {
  clearStateForLoad,
  loadCorpusFromProjectSources,
  registerFilesForActiveProject,
  registerFolderForActiveProject,
  renderProjectEmptyState,
} from "./corpus";
import { dom } from "./dom";
import { loadProjectSourcesForActiveProject, projectSourceTotalCount } from "./sources";
import { state, type ActiveProject } from "./state";
import type { CreateCorpusProjectRequest } from "./generated/CreateCorpusProjectRequest.js";
import {
  PROJECT_LANGUAGE_OPTIONS,
  defaultProjectNameFromPath,
  normaliseProjectDescription,
  projectCreationMetadataIsValid,
  projectLanguageFromValue,
} from "./project-creation-dialog";
import { clearDecisions, loadDecisionsForProject } from "./repeated-artifacts";
import { formatRemovalRuleMigrationSummary } from "./removal-rule-migration.ts";
import { hideWorkbenchSwitcher, resetWorkbenchForProject } from "./workbench";

interface RecentProject {
  name: string;
  rootPath: string;
  lastOpenedAt: string;
}

interface RecentProjectsStore {
  version: 1;
  projects: RecentProject[];
}

interface ProjectCreationSource {
  kind: "file" | "folder";
  path: string;
}

interface ProjectCreationDialogChoice {
  request: CreateCorpusProjectRequest;
  files: string[];
  folders: string[];
}

const RECENT_PROJECTS_KEY = "corpuswright.recentProjects.v1";
const MAX_RECENT_PROJECTS = 10;

let projectCreationRootPath = "";
let pendingProjectCreationSources: ProjectCreationSource[] = [];
let projectCreationResolve: ((choice: ProjectCreationDialogChoice | null) => void) | null = null;

function isRecentProject(value: unknown): value is RecentProject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.name === "string" &&
    typeof obj.rootPath === "string" &&
    typeof obj.lastOpenedAt === "string"
  );
}

function readRecentProjects(): RecentProject[] {
  const raw = localStorage.getItem(RECENT_PROJECTS_KEY);
  if (!raw) {
    return [];
  }

  try {
    const parsed = JSON.parse(raw) as unknown;
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      return [];
    }
    const store = parsed as Record<string, unknown>;
    if (store.version !== 1 || !Array.isArray(store.projects)) {
      return [];
    }
    return store.projects.filter(isRecentProject).slice(0, MAX_RECENT_PROJECTS);
  } catch {
    return [];
  }
}

function writeRecentProjects(projects: RecentProject[]): void {
  const store: RecentProjectsStore = {
    version: 1,
    projects: projects.slice(0, MAX_RECENT_PROJECTS),
  };
  localStorage.setItem(RECENT_PROJECTS_KEY, JSON.stringify(store));
}

function recordRecentProject(project: ActiveProject): void {
  const now = new Date().toISOString();
  const projects = readRecentProjects().filter((recent) => recent.rootPath !== project.rootPath);
  projects.unshift({
    name: project.name,
    rootPath: project.rootPath,
    lastOpenedAt: now,
  });
  writeRecentProjects(projects);
  renderRecentProjects();
}

function removeRecentProject(rootPath: string): void {
  writeRecentProjects(readRecentProjects().filter((project) => project.rootPath !== rootPath));
  renderRecentProjects();
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function setWelcomeStatus(message: string, isError = false): void {
  dom.welcomeStatus.textContent = message;
  dom.welcomeStatus.classList.toggle("is-error", isError);
}

function closeMenus(): void {
  document.querySelectorAll(".dropdown").forEach((dropdown) => {
    dropdown.classList.remove("is-open");
  });
}

function showWelcome(message = ""): void {
  dom.welcomeScreen.classList.remove("hidden");
  dom.projectWorkspace.classList.add("hidden");
  dom.menuCloseProject.classList.add("hidden");
  dom.projectContext.classList.add("hidden");
  hideWorkbenchSwitcher();
  setWelcomeStatus(message);
  renderRecentProjects();
  closeMenus();
}

async function activateProject(project: ActiveProject, scanSourceRoot: boolean): Promise<void> {
  state.activeProject = project;
  state.analysisCorpus = null;
  const decisionMigration = await loadDecisionsForProject(project.rootPath);
  dom.welcomeScreen.classList.add("hidden");
  dom.projectWorkspace.classList.remove("hidden");
  dom.menuCloseProject.classList.remove("hidden");
  dom.projectContext.classList.remove("hidden");
  dom.projectContextName.textContent = `Project: ${project.name}`;
  dom.projectContextPath.textContent = project.rootPath;

  state.projectSources = [];
  state.projectSourceSummary = null;
  state.projectSourceQuerySummary = null;
  state.projectSourceQueryPending = true;
  state.sourceSearchQuery = "";
  dom.searchInput.value = "";
  clearStateForLoad();
  renderProjectEmptyState();
  resetWorkbenchForProject();
  closeMenus();

  await loadProjectSourcesForActiveProject();
  if (scanSourceRoot && project.sourceRoots.length > 0 && projectSourceTotalCount() === 0) {
    await registerFolderForActiveProject(project.sourceRoots[0]);
  } else {
    await loadCorpusFromProjectSources();
    if (projectSourceTotalCount() > 0) {
      dom.statusBar.textContent = `Project opened: ${project.name}`;
    }
  }
  const migrationStatus = formatRemovalRuleMigrationSummary(decisionMigration);
  if (migrationStatus) {
    dom.statusBar.textContent = `${dom.statusBar.textContent} ${migrationStatus}`;
  }
}

async function chooseProjectFolder(cancelMessage: string): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    setWelcomeStatus(cancelMessage);
    return null;
  }
  return selected;
}

function renderProjectLanguageOptions(): void {
  dom.projectCreateLanguage.replaceChildren(
    ...PROJECT_LANGUAGE_OPTIONS.map((language) => {
      const option = document.createElement("option");
      option.value = language;
      option.textContent = language;
      return option;
    }),
  );
  dom.projectCreateLanguage.selectedIndex = -1;
}

function updateProjectCreationConfirmState(): void {
  dom.projectCreateConfirm.disabled = !projectCreationMetadataIsValid({
    name: dom.projectCreateName.value,
    language: dom.projectCreateLanguage.value,
  });
}

function renderProjectCreationSources(): void {
  dom.projectCreateSourcesList.replaceChildren();
  const count = pendingProjectCreationSources.length;
  dom.projectCreateSourceSummary.textContent =
    count === 0
      ? "No initial sources selected."
      : `${count.toLocaleString()} initial ${count === 1 ? "source" : "sources"} selected.`;

  for (const source of pendingProjectCreationSources) {
    const row = document.createElement("div");
    row.className = "project-create-source-row";

    const details = document.createElement("div");
    details.className = "project-create-source-path";
    details.title = source.path;
    details.textContent = source.path;

    const kind = document.createElement("span");
    kind.className = "project-create-source-kind";
    kind.textContent = source.kind === "file" ? "File" : "Folder";
    details.appendChild(kind);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "secondary-btn";
    remove.textContent = "Remove";
    remove.addEventListener("click", () => {
      pendingProjectCreationSources = pendingProjectCreationSources.filter(
        (candidate) => candidate.path !== source.path,
      );
      renderProjectCreationSources();
    });

    row.append(details, remove);
    dom.projectCreateSourcesList.appendChild(row);
  }
}

function addProjectCreationSources(kind: ProjectCreationSource["kind"], paths: string[]): void {
  const existing = new Set(pendingProjectCreationSources.map((source) => source.path));
  for (const path of paths) {
    if (existing.has(path)) {
      continue;
    }
    existing.add(path);
    pendingProjectCreationSources.push({ kind, path });
  }
  renderProjectCreationSources();
}

async function chooseProjectCreationFiles(): Promise<void> {
  const selected = await open({
    directory: false,
    multiple: true,
  });
  if (selected === null) {
    return;
  }
  addProjectCreationSources("file", Array.isArray(selected) ? selected : [selected]);
}

async function chooseProjectCreationFolder(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    return;
  }
  addProjectCreationSources("folder", [selected]);
}

function resetProjectCreationDialog(rootPath: string): void {
  projectCreationRootPath = rootPath;
  pendingProjectCreationSources = [];
  dom.projectCreateName.value = defaultProjectNameFromPath(rootPath);
  renderProjectLanguageOptions();
  dom.projectCreateDescription.value = "";
  dom.projectCreateStatus.textContent = "";
  renderProjectCreationSources();
  updateProjectCreationConfirmState();
}

function selectedProjectCreationChoice(): ProjectCreationDialogChoice | null {
  const language = projectLanguageFromValue(dom.projectCreateLanguage.value);
  if (
    !projectCreationMetadataIsValid({
      name: dom.projectCreateName.value,
      language: dom.projectCreateLanguage.value,
    }) ||
    language === null
  ) {
    dom.projectCreateStatus.textContent = "Project name and language are required.";
    updateProjectCreationConfirmState();
    return null;
  }

  const files = pendingProjectCreationSources
    .filter((source) => source.kind === "file")
    .map((source) => source.path);
  const folders = pendingProjectCreationSources
    .filter((source) => source.kind === "folder")
    .map((source) => source.path);
  const request: CreateCorpusProjectRequest = {
    rootPath: projectCreationRootPath,
    name: dom.projectCreateName.value.trim(),
    language,
    description: normaliseProjectDescription(dom.projectCreateDescription.value),
  };

  return { request, files, folders };
}

function closeProjectCreationDialog(choice: ProjectCreationDialogChoice | null): void {
  dom.projectCreateModal.classList.add("hidden");
  const resolve = projectCreationResolve;
  projectCreationResolve = null;
  resolve?.(choice);
}

function showProjectCreationDialog(rootPath: string): Promise<ProjectCreationDialogChoice | null> {
  if (projectCreationResolve) {
    closeProjectCreationDialog(null);
  }
  resetProjectCreationDialog(rootPath);
  dom.projectCreateModal.classList.remove("hidden");
  dom.projectCreateName.focus();

  return new Promise((resolve) => {
    projectCreationResolve = resolve;
  });
}

async function importInitialProjectSources(choice: ProjectCreationDialogChoice): Promise<void> {
  if (choice.files.length > 0) {
    await registerFilesForActiveProject(choice.files);
  }
  for (const folder of choice.folders) {
    await registerFolderForActiveProject(folder);
  }
}

async function openProjectFromPath(path: string, scanSourceRoot: boolean): Promise<void> {
  const project = await invoke<ActiveProject>("open_corpus_project_command", { path });
  recordRecentProject(project);
  await activateProject(project, scanSourceRoot && project.sourceRoots.length > 0);
}

async function handleNewProject(): Promise<void> {
  const selected = await chooseProjectFolder("New project cancelled.");
  if (selected === null) {
    return;
  }
  const choice = await showProjectCreationDialog(selected);
  if (choice === null) {
    setWelcomeStatus("New project cancelled.");
    return;
  }

  try {
    setWelcomeStatus("Creating project...");
    const project = await invoke<ActiveProject>("create_corpus_project_command", { request: choice.request });
    recordRecentProject(project);
    await activateProject(project, false);
    await importInitialProjectSources(choice);
  } catch (error) {
    setWelcomeStatus(formatError(error), true);
  }
}

async function handleOpenProject(): Promise<void> {
  const selected = await chooseProjectFolder("Open project cancelled.");
  if (selected === null) {
    return;
  }

  try {
    setWelcomeStatus("Opening project...");
    await openProjectFromPath(selected, true);
  } catch (error) {
    setWelcomeStatus(formatError(error), true);
  }
}

async function handleOpenFolderAsProject(): Promise<void> {
  const selected = await chooseProjectFolder("Open folder as project cancelled.");
  if (selected === null) {
    return;
  }

  try {
    setWelcomeStatus("Creating project metadata...");
    const project = await invoke<ActiveProject>("initialise_folder_as_corpus_project_command", {
      rootPath: selected,
    });
    recordRecentProject(project);
    await activateProject(project, true);
  } catch (error) {
    setWelcomeStatus(formatError(error), true);
  }
}

async function handleRecentProjectOpen(project: RecentProject): Promise<void> {
  try {
    setWelcomeStatus("Opening project...");
    await openProjectFromPath(project.rootPath, true);
  } catch (error) {
    setWelcomeStatus(formatError(error), true);
  }
}

async function handleCloseProject(): Promise<void> {
  try {
    await invoke("clear_corpus_command");
  } catch (error) {
    dom.statusBar.textContent = `Close project warning: ${formatError(error)}`;
  }
  clearDecisions();
  state.activeProject = null;
  state.analysisCorpus = null;
  state.projectSources = [];
  state.projectSourceSummary = null;
  state.projectSourceQuerySummary = null;
  state.projectSourceQueryPending = false;
  state.sourceSearchQuery = "";
  dom.searchInput.value = "";
  clearStateForLoad();
  dom.corpusSummary.replaceChildren();
  dom.statusBar.textContent = "Ready";
  showWelcome("Project closed.");
}

function renderRecentProjects(): void {
  dom.recentProjectsList.replaceChildren();
  const projects = readRecentProjects();
  if (projects.length === 0) {
    const empty = document.createElement("div");
    empty.className = "recent-projects-empty";
    empty.textContent = "No recent projects yet.";
    dom.recentProjectsList.appendChild(empty);
    return;
  }

  for (const project of projects) {
    const row = document.createElement("div");
    row.className = "recent-project-row";

    const openButton = document.createElement("button");
    openButton.type = "button";
    openButton.className = "recent-project-open";
    openButton.addEventListener("click", () => {
      handleRecentProjectOpen(project);
    });

    const name = document.createElement("span");
    name.className = "recent-project-name";
    name.textContent = project.name;

    const path = document.createElement("span");
    path.className = "recent-project-path";
    path.textContent = project.rootPath;

    openButton.append(name, path);

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.className = "recent-project-remove";
    removeButton.textContent = "Remove";
    removeButton.addEventListener("click", () => {
      removeRecentProject(project.rootPath);
    });

    row.append(openButton, removeButton);
    dom.recentProjectsList.appendChild(row);
  }
}

export function initProjectEntry(): void {
  dom.btnNewProject.addEventListener("click", () => {
    handleNewProject();
  });
  dom.btnOpenProject.addEventListener("click", () => {
    handleOpenProject();
  });
  dom.btnOpenFolderProject.addEventListener("click", () => {
    handleOpenFolderAsProject();
  });
  dom.menuNewProject.addEventListener("click", () => {
    handleNewProject();
  });
  dom.menuOpenProject.addEventListener("click", () => {
    handleOpenProject();
  });
  dom.menuOpenFolderProject.addEventListener("click", () => {
    handleOpenFolderAsProject();
  });
  dom.menuCloseProject.addEventListener("click", () => {
    handleCloseProject();
  });
  dom.projectCreateName.addEventListener("input", () => {
    updateProjectCreationConfirmState();
  });
  dom.projectCreateLanguage.addEventListener("change", () => {
    updateProjectCreationConfirmState();
  });
  dom.projectCreateAddFiles.addEventListener("click", () => {
    chooseProjectCreationFiles();
  });
  dom.projectCreateAddFolder.addEventListener("click", () => {
    chooseProjectCreationFolder();
  });
  dom.projectCreateCancel.addEventListener("click", () => {
    closeProjectCreationDialog(null);
  });
  dom.projectCreateCloseTop.addEventListener("click", () => {
    closeProjectCreationDialog(null);
  });
  dom.projectCreateConfirm.addEventListener("click", () => {
    const choice = selectedProjectCreationChoice();
    if (choice) {
      closeProjectCreationDialog(choice);
    }
  });

  showWelcome();
}
