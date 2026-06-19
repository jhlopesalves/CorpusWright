import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import {
  clearStateForLoad,
  loadCorpusFromProjectSources,
  registerFolderForActiveProject,
  renderProjectEmptyState,
} from "./corpus";
import { dom } from "./dom";
import { loadProjectSourcesForActiveProject, projectSourceTotalCount } from "./sources";
import { state, type ActiveProject } from "./state";
import { clearDecisions, loadDecisionsForProject } from "./repeated-artifacts";
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

const RECENT_PROJECTS_KEY = "corpuswright.recentProjects.v1";
const MAX_RECENT_PROJECTS = 10;

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
  await loadDecisionsForProject(project.rootPath);
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

  try {
    setWelcomeStatus("Creating project...");
    const project = await invoke<ActiveProject>("create_corpus_project_command", { rootPath: selected });
    recordRecentProject(project);
    await activateProject(project, false);
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

  showWelcome();
}
