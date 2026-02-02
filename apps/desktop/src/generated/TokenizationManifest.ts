import type { TokenizationConfig } from "./TokenizationConfig.js";

export type TokenizationManifest = { id: string, version: string, unicodeSegmentationVersion: string, config: TokenizationConfig, };
