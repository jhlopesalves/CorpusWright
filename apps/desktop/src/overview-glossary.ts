export interface OverviewGlossaryEntry {
  id: string;
  label: string;
  explanation: string;
  latex: string;
}

export const overviewGlossary = {
  giniCoefficient: {
    id: "gini-coefficient",
    label: "Gini coefficient",
    explanation: "Summarises inequality in document lengths: 0 means equal lengths, while larger values mean greater imbalance.",
    latex: String.raw`G=\frac{\sum_{i=1}^{n}\sum_{j=1}^{n}\lvert x_i-x_j\rvert}{2n\sum_{i=1}^{n}x_i}`,
  },
  coefficientOfVariation: {
    id: "coefficient-of-variation",
    label: "Coefficient of variation",
    explanation: "Compares the spread of document lengths with their mean length.",
    latex: String.raw`\mathrm{CV}=\frac{\sigma_x}{\mu_x}`,
  },
  effectiveDocuments: {
    id: "effective-documents",
    label: "Effective documents",
    explanation: "Expresses document balance as the equivalent number of equally sized documents.",
    latex: String.raw`N_{\mathrm{eff}}=\frac{1}{\sum_{d=1}^{D}s_d^2}`,
  },
  hapax: {
    id: "hapax",
    label: "Hapax",
    explanation: "A type that occurs exactly once in the analysis corpus.",
    latex: String.raw`f(w)=1`,
  },
  disLegomena: {
    id: "dis-legomena",
    label: "Dis legomena",
    explanation: "Types that occur exactly twice in the analysis corpus.",
    latex: String.raw`f(w)=2`,
  },
  rawTtr: {
    id: "raw-ttr",
    label: "Raw type–token ratio (TTR)",
    explanation: "Divides unique types by tokens; it usually falls as a text becomes longer.",
    latex: String.raw`\mathrm{TTR}=\frac{V}{N}`,
  },
  mattr: {
    id: "mattr",
    label: "MATTR",
    explanation: "Averages type–token ratios across overlapping fixed-size token windows within each document.",
    latex: String.raw`\mathrm{MATTR}_W=\frac{1}{N-W+1}\sum_{j=1}^{N-W+1}\frac{V_j}{W}`,
  },
  mtld: {
    id: "mtld",
    label: "MTLD",
    explanation: "Estimates the mean length of token runs that keep their running TTR above a threshold.",
    latex: String.raw`\mathrm{MTLD}=\frac{N}{F+F_{\mathrm{partial}}}`,
  },
  normalisedEntropy: {
    id: "normalised-entropy",
    label: "Normalised entropy / evenness",
    explanation: "Measures how evenly token mass is distributed across types, from 0 (concentrated) to 1 (perfectly even).",
    latex: String.raw`J=\frac{H}{\log_2 V},\qquad H=-\sum_{i=1}^{V}p_i\log_2 p_i`,
  },
  effectiveVocabulary: {
    id: "effective-vocabulary",
    label: "Effective vocabulary (entropy-based)",
    explanation: "Gives the number of equally frequent types that would have the same entropy as this corpus.",
    latex: String.raw`V_{\mathrm{eff}}=2^H`,
  },
  griesDp: {
    id: "gries-dp",
    label: "Gries DP",
    explanation: "Compares a type’s distribution across documents with the documents’ shares of corpus tokens.",
    latex: String.raw`\mathrm{DP}=\frac{1}{2}\sum_{d=1}^{D}\lvert p_d-s_d\rvert`,
  },
  documentFrequencySpectrum: {
    id: "document-frequency-spectrum",
    label: "Document-frequency spectrum",
    explanation: "Groups types by the number of documents in which each type occurs.",
    latex: String.raw`\mathrm{df}(w)=\left\lvert\{d:f_d(w)>0\}\right\rvert`,
  },
} as const satisfies Record<string, OverviewGlossaryEntry>;

export type OverviewGlossaryId = keyof typeof overviewGlossary;
