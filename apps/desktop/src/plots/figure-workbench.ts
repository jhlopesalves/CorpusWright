export type FigurePaletteMode = "theme" | "colorBlind" | "monochrome";
export type FigureControlValue = string | number | boolean;
export type FigureControlState = Record<string, FigureControlValue>;
export type FigureLoadState = "idle" | "loading" | "ready" | "empty" | "error";

export const FIGURE_PALETTE_STORAGE_KEY = "corpuswright.overview.figurePalette";
export const DEFAULT_FIGURE_PALETTE: FigurePaletteMode = "theme";

export interface StorageReader {
  getItem(key: string): string | null;
}

export interface StorageWriter {
  setItem(key: string, value: string): void;
}

export interface FigureControllerSnapshot<TState extends FigureControlState> {
  controls: Readonly<TState>;
  pins: ReadonlySet<string>;
  loadState: FigureLoadState;
  error: string | null;
}

export interface FigureControllerOptions<TState extends FigureControlState> {
  initialControls: TState;
  exportMetadata?: (controls: Readonly<TState>) => string[];
}

export class OverviewFigureController<TState extends FigureControlState = FigureControlState> {
  private controlsValue: TState;
  private readonly pinKeys = new Set<string>();
  private requestRevision = 0;
  private loadStateValue: FigureLoadState = "idle";
  private errorValue: string | null = null;
  private readonly listeners = new Set<(snapshot: FigureControllerSnapshot<TState>) => void>();
  private readonly metadata: (controls: Readonly<TState>) => string[];

  constructor(options: FigureControllerOptions<TState>) {
    this.controlsValue = { ...options.initialControls };
    this.metadata = options.exportMetadata ?? (() => []);
  }

  get controls(): Readonly<TState> {
    return this.controlsValue;
  }

  get pins(): ReadonlySet<string> {
    return this.pinKeys;
  }

  get loadState(): FigureLoadState {
    return this.loadStateValue;
  }

  get error(): string | null {
    return this.errorValue;
  }

  updateControls(patch: Partial<TState>): void {
    this.controlsValue = { ...this.controlsValue, ...patch };
    this.emit();
  }

  resetControls(next: TState): void {
    this.controlsValue = { ...next };
    this.emit();
  }

  isPinned(key: string): boolean {
    return this.pinKeys.has(key);
  }

  togglePin(key: string): boolean {
    if (!key) return false;
    const pinned = !this.pinKeys.delete(key);
    if (pinned) this.pinKeys.add(key);
    this.emit();
    return pinned;
  }

  clearPins(): void {
    if (this.pinKeys.size === 0) return;
    this.pinKeys.clear();
    this.emit();
  }

  exportMetadata(): string[] {
    return this.metadata(this.controlsValue).filter(Boolean);
  }

  subscribe(listener: (snapshot: FigureControllerSnapshot<TState>) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async loadLatest<TResult>(
    load: () => Promise<TResult>,
    apply: (result: TResult) => void,
    isEmpty: (result: TResult) => boolean = () => false,
  ): Promise<boolean> {
    const revision = ++this.requestRevision;
    this.loadStateValue = "loading";
    this.errorValue = null;
    this.emit();
    try {
      const result = await load();
      if (revision !== this.requestRevision) return false;
      apply(result);
      this.loadStateValue = isEmpty(result) ? "empty" : "ready";
      this.emit();
      return true;
    } catch (error) {
      if (revision !== this.requestRevision) return false;
      this.loadStateValue = "error";
      this.errorValue = error instanceof Error ? error.message : String(error);
      this.emit();
      return true;
    }
  }

  invalidatePendingLoad(): void {
    this.requestRevision += 1;
  }

  private emit(): void {
    const snapshot: FigureControllerSnapshot<TState> = {
      controls: this.controlsValue,
      pins: this.pinKeys,
      loadState: this.loadStateValue,
      error: this.errorValue,
    };
    for (const listener of this.listeners) listener(snapshot);
  }
}

export function readFigurePalette(storage: StorageReader = localStorage): FigurePaletteMode {
  try {
    const value = storage.getItem(FIGURE_PALETTE_STORAGE_KEY);
    return isFigurePaletteMode(value) ? value : DEFAULT_FIGURE_PALETTE;
  } catch {
    return DEFAULT_FIGURE_PALETTE;
  }
}

export function writeFigurePalette(
  mode: FigurePaletteMode,
  storage: StorageWriter = localStorage,
): void {
  try {
    storage.setItem(FIGURE_PALETTE_STORAGE_KEY, mode);
  } catch {
    // Figure palette persistence is best-effort.
  }
}

export function applyFigurePalette(root: HTMLElement, mode: FigurePaletteMode): void {
  root.dataset.figurePalette = mode;
}

export function figurePaletteLabel(mode: FigurePaletteMode): string {
  switch (mode) {
    case "theme": return "Theme";
    case "colorBlind": return "Colour-blind safe";
    case "monochrome": return "Monochrome / print";
  }
}

export function evidenceKey(...parts: Array<string | number>): string {
  return parts.map((part) => encodeURIComponent(String(part))).join(":");
}

function isFigurePaletteMode(value: unknown): value is FigurePaletteMode {
  return value === "theme" || value === "colorBlind" || value === "monochrome";
}
