export type FigurePaletteMode = "theme" | "colorBlind" | "monochrome";
export type FigureControlValue = string | number | boolean;
export type FigureControlState = Record<string, FigureControlValue>;
export type FigureLoadState = "idle" | "loading" | "ready" | "empty" | "error";
export type OverviewFigureId =
  | "overview-length-histogram"
  | "overview-mattr-distribution"
  | "overview-rank-frequency"
  | "overview-vocabulary-growth"
  | "overview-lexical-spectrum"
  | "overview-cumulative-coverage"
  | "overview-dispersion-profile"
  | "overview-dispersion-plane";

export interface FigureCapabilities {
  supportedViews: readonly string[];
  primaryControls: readonly string[];
  secondaryControls: readonly string[];
  fixedState: Readonly<FigureControlState>;
  compatiblePinPrefixes: readonly string[];
}

export interface FigureInteractionPolicy {
  id: OverviewFigureId;
  authoredDefault: Readonly<FigureControlState>;
  capabilities: FigureCapabilities;
  normalize(state: Readonly<FigureControlState>): FigureControlState;
  isPinCompatible(key: string, state: Readonly<FigureControlState>): boolean;
}

export interface FigureControllerSnapshot<TState extends FigureControlState> {
  controls: Readonly<TState>;
  pins: ReadonlySet<string>;
  loadState: FigureLoadState;
  error: string | null;
}

export interface OverviewFigureControllerContract {
  readonly pins: ReadonlySet<string>;
  togglePin(key: string): boolean;
  clearPins(): void;
  exportMetadata(): string[];
}

export interface FigureControllerOptions<TState extends FigureControlState> {
  initialControls: TState;
  exportMetadata?: (controls: Readonly<TState>) => string[];
  policy?: FigureInteractionPolicy;
}

export class OverviewFigureController<TState extends FigureControlState = FigureControlState> {
  private controlsValue: TState;
  private readonly pinKeys = new Set<string>();
  private requestRevision = 0;
  private loadStateValue: FigureLoadState = "idle";
  private errorValue: string | null = null;
  private readonly listeners = new Set<(snapshot: FigureControllerSnapshot<TState>) => void>();
  private readonly metadata: (controls: Readonly<TState>) => string[];
  private readonly policy?: FigureInteractionPolicy;

  constructor(options: FigureControllerOptions<TState>) {
    this.policy = options.policy;
    this.controlsValue = normalizeWithPolicy<TState>(options.policy, options.initialControls);
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
    this.controlsValue = normalizeWithPolicy<TState>(this.policy, { ...this.controlsValue, ...patch } as TState);
    this.removeIncompatiblePins();
    this.emit();
  }

  resetControls(next: TState): void {
    this.controlsValue = normalizeWithPolicy<TState>(this.policy, next);
    this.removeIncompatiblePins();
    this.emit();
  }

  isPinned(key: string): boolean {
    return this.pinKeys.has(key);
  }

  togglePin(key: string): boolean {
    if (!key || this.policy && !this.policy.isPinCompatible(key, this.controlsValue)) return false;
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
    this.controlsValue = normalizeWithPolicy<TState>(this.policy, this.controlsValue);
    this.removeIncompatiblePins();
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

  private removeIncompatiblePins(): void {
    if (!this.policy) return;
    for (const key of this.pinKeys) {
      if (!this.policy.isPinCompatible(key, this.controlsValue)) this.pinKeys.delete(key);
    }
  }
}

export const FIGURE_INTERACTION_POLICIES: Readonly<Record<OverviewFigureId, FigureInteractionPolicy>> = {
  "overview-length-histogram": distributionPolicy("overview-length-histogram", true),
  "overview-mattr-distribution": distributionPolicy("overview-mattr-distribution", false),
  "overview-rank-frequency": viewPolicy("overview-rank-frequency", ["curve", "residuals"], "curve", ["rank:"]),
  "overview-vocabulary-growth": viewPolicy("overview-vocabulary-growth", ["growth", "heaps"], "growth", ["growth:"]),
  "overview-lexical-spectrum": fixedPolicy("overview-lexical-spectrum", ["spectrum:", "lexical-"]),
  "overview-cumulative-coverage": policy({
    id: "overview-cumulative-coverage",
    authoredDefault: { target: "0.5" },
    supportedViews: [],
    primaryControls: ["target"],
    fixedState: { xMode: "topK", labels: "milestones" },
    compatiblePinPrefixes: ["coverage:", "coverage-"],
    normalize: (state) => ({ target: member(state.target, ["0.5", "0.8", "0.9", "0.95"], "0.5") }),
  }),
  "overview-dispersion-profile": fixedPolicy("overview-dispersion-profile", ["dispersion-reach-"]),
  "overview-dispersion-plane": fixedPolicy("overview-dispersion-plane", ["plane:"]),
};

export function normalizeFigureState(
  figureId: OverviewFigureId,
  state: Readonly<FigureControlState>,
): FigureControlState {
  return FIGURE_INTERACTION_POLICIES[figureId].normalize(state);
}

function normalizeWithPolicy<TState extends FigureControlState>(
  selectedPolicy: FigureInteractionPolicy | undefined,
  state: Readonly<TState>,
): TState {
  return { ...(selectedPolicy ? selectedPolicy.normalize(state) : state) } as TState;
}

function distributionPolicy(id: Extract<OverviewFigureId, "overview-length-histogram" | "overview-mattr-distribution">, length: boolean): FigureInteractionPolicy {
  return policy({
    id,
    authoredDefault: { view: "histogram", histogramRule: "auto", manualBins: 24, xScale: length ? "log" : "linear" },
    supportedViews: ["histogram", "ecdf", "qq"],
    primaryControls: ["view"],
    secondaryControls: length ? ["histogramRule", "manualBins", "xScale"] : ["histogramRule", "manualBins"],
    fixedState: {
      yMode: "share",
      qqReference: length ? "lognormal" : "normal",
      ...(length ? {} : { xScale: "linear" }),
    },
    compatiblePinPrefixes: ["distribution-bin:", "distribution-qq:", "distribution-witness:", "mattr-"],
    normalize: (state) => ({
      view: member(state.view, ["histogram", "ecdf", "qq"], "histogram"),
      histogramRule: member(state.histogramRule, ["auto", "manual"], "auto"),
      manualBins: clampInteger(state.manualBins, 8, 100, 24),
      xScale: length ? member(state.xScale, ["log", "linear"], "log") : "linear",
    }),
    isPinCompatible: (key, state) => {
      if (key.startsWith("distribution-bin:")) return state.view === "histogram";
      if (key.startsWith("distribution-qq:")) return state.view === "qq";
      if (key.startsWith("distribution-witness:") || key.startsWith("mattr-")) return !length && state.view === "histogram";
      return false;
    },
  });
}

function viewPolicy(
  id: Extract<OverviewFigureId, "overview-rank-frequency" | "overview-vocabulary-growth">,
  views: readonly string[],
  authoredView: string,
  compatiblePinPrefixes: readonly string[],
): FigureInteractionPolicy {
  return policy({
    id,
    authoredDefault: { view: authoredView },
    supportedViews: views,
    primaryControls: ["view"],
    fixedState: {},
    compatiblePinPrefixes,
    normalize: (state) => ({ view: member(state.view, views, authoredView) }),
  });
}

function fixedPolicy(id: OverviewFigureId, compatiblePinPrefixes: readonly string[]): FigureInteractionPolicy {
  return policy({
    id,
    authoredDefault: {},
    supportedViews: [],
    primaryControls: [],
    fixedState: {},
    compatiblePinPrefixes,
    normalize: () => ({}),
  });
}

function policy(options: {
  id: OverviewFigureId;
  authoredDefault: FigureControlState;
  supportedViews: readonly string[];
  primaryControls: readonly string[];
  secondaryControls?: readonly string[];
  fixedState: FigureControlState;
  compatiblePinPrefixes: readonly string[];
  normalize: (state: Readonly<FigureControlState>) => FigureControlState;
  isPinCompatible?: (key: string, state: Readonly<FigureControlState>) => boolean;
}): FigureInteractionPolicy {
  return {
    id: options.id,
    authoredDefault: options.authoredDefault,
    capabilities: {
      supportedViews: options.supportedViews,
      primaryControls: options.primaryControls,
      secondaryControls: options.secondaryControls ?? [],
      fixedState: options.fixedState,
      compatiblePinPrefixes: options.compatiblePinPrefixes,
    },
    normalize: options.normalize,
    isPinCompatible: options.isPinCompatible ?? ((key) => options.compatiblePinPrefixes.some((prefix) => key.startsWith(prefix))),
  };
}

function member<T extends string>(value: FigureControlValue | undefined, values: readonly T[], fallback: T): T {
  return typeof value === "string" && values.includes(value as T) ? value as T : fallback;
}

function clampInteger(value: FigureControlValue | undefined, minimum: number, maximum: number, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(maximum, Math.max(minimum, Math.round(value)));
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
