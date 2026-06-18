import { invoke } from "@tauri-apps/api/core";

import type { CleaningConfig } from "./generated/CleaningConfig.js";

export type AnalysisInputSource = "processed" | "extracted";

export interface AnalysisInputDocument {
  sourcePath: string;
  relativePath: string;
  text: string;
}

export interface AnalysisInputTextsResult {
  documents: AnalysisInputDocument[];
  skippedDocuments: number;
  skippedPdfDocuments: number;
}

export interface AnalysisDocument {
  sourcePath: string;
  relativePath: string;
  tokenCount: number;
  termCounts: Record<string, number>;
}

export interface AnalysisTerm {
  token: string;
  frequency: number;
  documentFrequency: number;
  ratePer10k: number;
}

export interface AnalysisCorpus {
  documents: AnalysisDocument[];
  sourceCount: number;
  totalTokens: number;
  totalTypes: number;
  terms: AnalysisTerm[];
  inputTextSource: AnalysisInputSource;
  builtAt: string;
  buildSignature: string;
}

interface MutableTerm {
  frequency: number;
  documentFrequency: number;
}

export function loadAnalysisInputTexts(
  indices: number[],
  corpusVersion: number,
  useProcessedText: boolean,
  cleaningConfig: CleaningConfig,
): Promise<AnalysisInputTextsResult> {
  return invoke<AnalysisInputTextsResult>("analysis_input_texts_command", {
    indices,
    corpusVersion,
    useProcessedText,
    cleaningConfig,
  });
}

export function tokeniseAnalysisText(text: string): string[] {
  const normalised = text.normalize("NFC").toLowerCase();
  const pattern = /[\p{L}\p{N}]+(?:['’\-][\p{L}\p{N}]+)*/gu;
  return Array.from(normalised.matchAll(pattern), (match) => match[0]);
}

export function buildAnalysisCorpus(
  inputDocuments: AnalysisInputDocument[],
  inputTextSource: AnalysisInputSource,
  buildSignature: string,
): AnalysisCorpus {
  const terms = new Map<string, MutableTerm>();
  const documents: AnalysisDocument[] = [];

  for (const document of inputDocuments) {
    const tokens = tokeniseAnalysisText(document.text);
    const documentTerms = new Map<string, number>();

    for (const token of tokens) {
      documentTerms.set(token, (documentTerms.get(token) ?? 0) + 1);
    }

    for (const [token, count] of documentTerms) {
      const term = terms.get(token) ?? { frequency: 0, documentFrequency: 0 };
      term.frequency += count;
      term.documentFrequency += 1;
      terms.set(token, term);
    }

    documents.push({
      sourcePath: document.sourcePath,
      relativePath: document.relativePath,
      tokenCount: tokens.length,
      termCounts: Object.fromEntries(documentTerms),
    });
  }

  const totalTokens = documents.reduce((sum, document) => sum + document.tokenCount, 0);
  const analysisTerms: AnalysisTerm[] = Array.from(terms.entries(), ([token, term]) => ({
    token,
    frequency: term.frequency,
    documentFrequency: term.documentFrequency,
    ratePer10k: totalTokens > 0 ? (term.frequency / totalTokens) * 10_000 : 0,
  })).sort((a, b) => b.frequency - a.frequency || a.token.localeCompare(b.token));

  return {
    documents,
    sourceCount: new Set(documents.map((document) => document.sourcePath)).size,
    totalTokens,
    totalTypes: analysisTerms.length,
    terms: analysisTerms,
    inputTextSource,
    builtAt: new Date().toISOString(),
    buildSignature,
  };
}
