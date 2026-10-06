export interface OverviewGlossaryEntry {
  id: string;
  label: string;
  explanation: string;
  latex: string;
  detail?: string;
  symbols?: Array<{ sym: string; def: string }>;
  interpretation?: string;
  limitations?: string[];
  example?: string;
}

export const overviewGlossary = {
  token: {
    id: "token",
    label: "Token",
    explanation:
      "A countable word-like unit: one occurrence of a word or number in the analysis corpus.",
    latex: String.raw`N=\sum_{d=1}^{D}N_d`,
    detail:
      "CorpusWright segments text into Unicode word segments and counts two kinds: words and numbers. Standalone punctuation is never counted. By default, hyphen compounds (state-of-the-art) and apostrophe contractions (don't) are kept whole, and numeric separators are split. Every corpus-level figure on this page counts these tokens.",
    symbols: [
      { sym: String.raw`N`, def: "Total tokens in the analysis corpus." },
      { sym: String.raw`N_d`, def: "Tokens in document d." },
      { sym: String.raw`D`, def: "Number of documents." },
    ],
    interpretation: "Tokens measure the amount of counted text. Per-million frequency uses N as its denominator; a type share uses V and a document share uses the stated document total.",
    limitations: [
      "Counts depend on the tokenisation rules above; comparisons must account for differences in processing.",
      "Number-heavy material contributes to N under this word-and-number policy.",
    ],
  },
  type: {
    id: "type",
    label: "Type",
    explanation:
      "A distinct token form. Counting every occurrence gives tokens; counting each distinct form once gives types.",
    latex: String.raw`V=\bigl\lvert\{\mathrm{key}(t):t\in\text{corpus}\}\bigr\rvert`,
    detail:
      "Two tokens belong to the same type when they share an aggregation key: the token's text, Unicode-normalised (NFC) and lowercased. 'The' and 'the' are one type; 'run' and 'running' remain two - types are surface forms, not lemmas. The Overview always computes over the full type universe of the analysis corpus.",
    symbols: [
      { sym: String.raw`V`, def: "Vocabulary size - the number of distinct types." },
      { sym: String.raw`\mathrm{key}(t)`, def: "Token t's aggregation key: NFC-normalised, lowercased text." },
    ],
    interpretation:
      "Types measure vocabulary breadth. The Frequency List applies its own view filters (alphabetic words only, hidden stopwords), so its displayed type count can legitimately differ from the Overview's total. The Overview is a fixed corpus snapshot; the Frequency List is an interactive filtered table.",
    limitations: [
      "Types are NFC-normalised, lowercased surface forms, not lemmas - inflected variants count separately.",
      "OCR errors and spelling variants can create additional observed types.",
    ],
  },
  documentUnit: {
    id: "document",
    label: "Document",
    explanation: "One source text in the analysis corpus: the unit behind document-size concentration, dispersion and document frequency.",
    latex: String.raw`s_d=\frac{N_d}{N}`,
    detail: "Every included, non-empty source file is one document. Document-size metrics compare token totals; dispersion compares a type's occurrence shares with document token shares. Empty or failed sources do not enter the analysis population.",
    symbols: [
      { sym: String.raw`s_d`, def: "Document d's share of all corpus tokens." },
    ],
    interpretation: "Merging or splitting files changes the document partition. Read the largest-document share in Document-size concentration before interpreting document distribution.",
    limitations: [
      "Merging or splitting source files can change document-based measures without changing a word of text.",
    ],
  },
  giniCoefficient: {
    id: "gini-coefficient",
    label: "Gini coefficient",
    explanation: "Summarises inequality in document token counts: 0 means equal counts; larger values mean greater inequality.",
    latex: String.raw`G=\frac{\sum_{i=1}^{n}\sum_{j=1}^{n}\lvert x_i-x_j\rvert}{2n\sum_{i=1}^{n}x_i}`,
    detail:
      "The Gini coefficient is the mean absolute difference in token length across all document pairs, divided by twice the mean length. Zero means equal token counts; larger values mean greater inequality. It does not describe differences in document content.",
    symbols: [
      { sym: String.raw`G`, def: "Gini coefficient for document lengths." },
      { sym: String.raw`x_i,\ x_j`, def: "Token lengths of documents i and j." },
      { sym: String.raw`n`, def: "Number of documents." },
      { sym: String.raw`\sum x_i`, def: "Total tokens across all documents." },
    ],
    interpretation: "Larger values mean greater inequality in token counts. Read Gini with the largest-document share, effective document count and feature dispersion. It does not assess corpus-design balance or representativeness.",
    limitations: [
      "Measures document-length inequality only, not content or quality.",
      "Unequal lengths are not inherently a problem - they are context for interpreting corpus-level frequencies.",
    ],
  },
  coefficientOfVariation: {
    id: "coefficient-of-variation",
    label: "Coefficient of variation",
    explanation: "Compares the spread of document lengths with their mean length.",
    latex: String.raw`\mathrm{CV}=\frac{\sigma_x}{\mu_x}`,
    detail:
      "The standard deviation of document lengths divided by their mean - a scale-free measure of how spread out document lengths are. Being relative to the mean, it can compare variability between corpora of different average sizes.",
    symbols: [
      { sym: String.raw`\mathrm{CV}`, def: "Coefficient of variation for document lengths." },
      { sym: String.raw`\sigma_x`, def: "Standard deviation of document lengths." },
      { sym: String.raw`\mu_x`, def: "Mean document length." },
    ],
    interpretation: "0 means equal document token counts; larger values mean greater spread relative to the mean. CorpusWright uses the population standard deviation of the included document lengths. There are no validated quality categories for this value.",
    limitations: [
      "Read with percentiles and concentration measures; CV alone does not describe the distribution's shape.",
      "Measures length only, not content.",
    ],
  },
  effectiveDocuments: {
    id: "effective-documents",
    label: "Effective document count",
    explanation: "Inverse squared token-share concentration, expressed as an equivalent count of equally sized documents.",
    latex: String.raw`D_{\mathrm{eff}}=\frac{1}{\sum_{d=1}^{D}s_d^2}`,
    detail: "For token shares s_d, calculate 1 divided by their sum of squares. The result is the equivalent count of equally sized documents with the same squared-share concentration; it need not be an integer.",
    symbols: [
      { sym: String.raw`D_{\mathrm{eff}}`, def: "Effective document count." },
      { sym: String.raw`s_d`, def: "Document d's share of total corpus tokens (its size as a fraction)." },
      { sym: String.raw`D`, def: "Number of documents." },
      { sym: String.raw`\sum s_d^2`, def: "Sum of squared shares - the Simpson concentration." },
    ],
    interpretation: "Equal token shares give D_eff = D. More concentrated shares give a smaller equivalent count. Four shares of 1/4 give 4; shares of 1/2, 1/6, 1/6 and 1/6 give 3.",
    limitations: [
      "This is not an inferential effective sample size or a count of independent observations.",
      "Document-size concentration does not establish corpus-design balance, representativeness or sampling adequacy.",
      "Depends on the document partition.",
    ],
  },
  hapax: {
    id: "hapax",
    label: "Hapax",
    explanation: "A type that occurs exactly once in the analysis corpus.",
    latex: String.raw`f(w)=1`,
    detail: "Hapax legomena occur exactly once in the prepared sample. Their share of types describes the observed low-frequency tail.",
    symbols: [
      { sym: String.raw`f(w)=1`, def: "Type w occurs exactly once." },
    ],
    interpretation: "Interpret the share with corpus size, morphology, names, terminology, composition and processing. A high share does not establish corpus inadequacy, saturation or representativeness. Inspect unexpected forms directly before attributing them to extraction errors.",
    limitations: [
      "Tokenisation, casing, cleaning, OCR and spelling variants can change the count.",
      "Depends on corpus size and design; similar sizes alone do not establish comparability.",
    ],
  },
  disLegomena: {
    id: "dis-legomena",
    label: "Dis legomena",
    explanation: "Types that occur exactly twice in the analysis corpus.",
    latex: String.raw`f(w)=2`,
    detail:
      "Dis legomena are types that occur exactly twice. With hapax, they describe the bottom of the frequency distribution - the rare-word tail.",
    symbols: [
      { sym: String.raw`f(w)=2`, def: "Type w occurs exactly twice." },
    ],
    interpretation: "Read frequency-two types alongside hapaxes and the remaining frequency classes. Their count changes as existing types recur and new types enter the sample; it need not decrease as the corpus grows.",
    limitations: [
      "Sensitive to tokenisation, casing and cleaning, like hapax.",
      "Depends on corpus size and design; similar sizes alone do not establish comparability.",
    ],
  },
  mattr: {
    id: "mattr",
    label: "MATTR",
    explanation: "Averages type–token ratios across overlapping same-size token windows within each document.",
    latex: String.raw`\mathrm{MATTR}_W=\frac{1}{N-W+1}\sum_{j=1}^{N-W+1}\frac{V_j}{W}`,
    detail: "CorpusWright slides a 100-token window one token at a time within each document and averages the window TTRs. A fixed window reduces raw TTR's length sensitivity without making the result independent of sequence or vocabulary distribution.",
    symbols: [
      { sym: String.raw`\mathrm{MATTR}_W`, def: "Moving-average TTR computed with window size W." },
      { sym: String.raw`W`, def: "Window size, in tokens." },
      { sym: String.raw`N`, def: "Number of tokens in the document." },
      { sym: String.raw`V_j`, def: "Number of distinct types in the window starting at position j." },
      { sym: String.raw`N-W+1`, def: "Number of overlapping windows." },
    ],
    interpretation: "Larger values mean more distinct types within the fixed window. Keep the window, tokenisation and aggregation policy consistent when comparing documents; composition and token sequence still matter.",
    limitations: [
      "Depends on the window size W; comparisons across windows require an explicit design.",
      "Requires documents at least as long as the window.",
      "Sensitive to tokenisation and casing.",
    ],
  },
  mtld: {
    id: "mtld",
    label: "MTLD",
    explanation: "Reports a factor-adjusted run length at a fixed running-TTR threshold.",
    latex: String.raw`\mathrm{MTLD}=\frac{N}{F+F_{\mathrm{partial}}}`,
    detail:
      "MTLD walks through the tokens accumulating a running type-token ratio. Each time the running TTR reaches or falls below 0.720, it closes a factor and resets. For each direction, divide total tokens by the sum of complete factors and the fractional final factor. CorpusWright averages the forward and backward results.",
    symbols: [
      { sym: String.raw`N`, def: "Number of tokens." },
      { sym: String.raw`F`, def: "Number of complete factors - runs whose running TTR fell to the 0.720 threshold." },
      {
        sym: String.raw`F_{\mathrm{partial}}`,
        def: "The fractional final factor for the leftover tokens that never reached the threshold.",
      },
    ],
    interpretation: "Larger values describe longer runs before running TTR reaches 0.720, with the partial-factor adjustment and forward/backward averaging. MTLD reduces some length sensitivity but remains conditional on its threshold, sequence, type definition and vocabulary distribution.",
    limitations: [
      "CorpusWright requires at least 100 tokens. This is an availability rule, not evidence of estimate stability at that length.",
      "The partial factor is (1 - final TTR) / (1 - 0.720); a pass with no positive factor is unavailable.",
      "The result averages forward and backward passes and depends on tokenisation and aggregation.",
    ],
  },
  distributionDiagnostics: {
    id: "distribution-diagnostics",
    label: "distribution diagnostics",
    explanation:
      "Histogram, empirical cumulative distribution, and Q–Q views answer different questions about a complete document-level distribution.",
    latex: String.raw`\widehat F(x)=\frac{1}{n}\sum_{i=1}^{n}\mathbf 1(x_i\le x),\qquad p_i=\frac{i-0.5}{n}`,
    detail:
      "A histogram groups observations into numeric intervals and therefore depends on its binning rule. An ECDF keeps exact cumulative counts at observed values; when display points are deterministically compressed, CorpusWright retains the first and final observations and reports the compression. A Q–Q diagnostic pairs ordered observations with reference quantiles to reveal departures from a chosen reference shape.",
    symbols: [
      { sym: String.raw`\widehat F(x)`, def: "Empirical cumulative share at or below x." },
      { sym: String.raw`n`, def: "Eligible documents." },
      { sym: String.raw`p_i`, def: "Plotting position for ordered observation i." },
      { sym: String.raw`\mathbf 1(\cdot)`, def: "Indicator equal to one when its condition is true." },
    ],
    interpretation:
      "Use the histogram to see mass by interval, the ECDF to read percentiles without choosing bins, and the Q–Q view to diagnose shape. A nearly straight Q–Q pattern indicates similarity to the named reference over the displayed range; it is not a pass/fail test.",
    limitations: [
      "Histogram appearance changes with the bin rule and active scale.",
      "A reference distribution is a comparison device, not a claim that the metric must follow it.",
      "Deterministic display compression preserves endpoints and ordering but does not draw every observation.",
    ],
  },
  exactLexicalSpectrum: {
    id: "exact-lexical-spectrum",
    label: "exact lexical spectrum",
    explanation: "Counts how many vocabulary types occur at each exact integer frequency.",
    latex: String.raw`V(f)=\left\lvert\{w:\operatorname{freq}(w)=f\}\right\rvert`,
    detail:
      "Each plotted point represents one observed integer frequency f. The horizontal axis is not a set of continuous bins: if no type occurs exactly f times, there is no point at f. Bars and smoothing would imply unsupported intervals or continuity, so CorpusWright uses points on logarithmic axes.",
    symbols: [
      { sym: String.raw`V(f)`, def: "Number of types occurring exactly f times." },
      { sym: String.raw`w`, def: "One vocabulary type." },
      { sym: String.raw`\operatorname{freq}(w)`, def: "Raw corpus frequency of type w." },
    ],
    interpretation:
      "The left edge exposes hapax and dis legomena; the thinning right tail shows how few types reach large frequencies. Type share describes vocabulary composition, not the share of running tokens carried by a class.",
    limitations: [
      "Both axes are logarithmic, so equal screen distances represent multiplicative changes.",
      "The display series may be deterministically bounded even though each returned point remains an exact frequency count.",
    ],
  },
  dispersionPlaneDensity: {
    id: "dispersion-plane-density",
    label: "dispersion-plane density",
    explanation:
      "Counts types within cells of logarithmic raw frequency and Gries DP, separating the population field from selected lexical witnesses.",
    latex: String.raw`c_{ij}=\left\lvert\{w:x_i\le\log_{10}f(w)<x_{i+1},\ y_j\le\mathrm{DP}(w)<y_{j+1}\}\right\rvert`,
    detail: "The density field represents the complete bounded grid returned by Rust. Log density changes colour contrast, not cell membership or raw counts. Witnesses are selected deterministically to illustrate positions in the plane, not as a random or representative sample.",
    symbols: [
      { sym: String.raw`c_{ij}`, def: "Type count in frequency cell i and DP cell j." },
      { sym: String.raw`f(w)`, def: "Raw frequency of type w." },
      { sym: String.raw`\mathrm{DP}(w)`, def: "Gries DP of type w." },
    ],
    interpretation:
      "Read horizontal position as frequency, vertical position as concentration, and colour strength as the number of types in that region. Witness marks sit above the density field as evidence, not as the population itself.",
    limitations: [
      "The grid fixes visual resolution; its cells are not natural vocabulary categories.",
      "The default view uses continuous coordinates without dividing reference lines or validated categories.",
    ],
  },
  shannonEntropy: {
    id: "shannon-entropy",
    label: "Shannon entropy",
    explanation:
      "The average uncertainty, in bits, of the type of a randomly drawn token.",
    latex: String.raw`H=-\sum_{i=1}^{V}p_i\log_2 p_i`,
    detail:
      "Shannon entropy averages -log2(p_i) over the empirical type distribution. It describes the type of a uniformly sampled corpus token, ignoring word order and context. For a fixed observed vocabulary, equal type frequencies maximise H.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (base-2 logarithm)." },
      { sym: String.raw`p_i`, def: "Relative frequency of type i - its count divided by total tokens." },
      { sym: String.raw`V`, def: "Number of distinct types." },
    ],
    interpretation: "For V observed types, the maximum is log2(V), reached at equal frequencies. Observed H depends on the full frequency distribution and need not rise whenever V rises. Normalised entropy and effective vocabulary derive from this same H.",
    limitations: [
      "Uses empirical frequencies in bits, without a finite-sample bias correction or uncertainty interval.",
      "Comparisons remain conditional on size, composition, sampling, tokenisation and type definition.",
    ],
  },
  zipfFit: {
    id: "zipf-fit",
    label: "Rank-frequency trend",
    explanation: "Summarises a selected log-frequency versus log-rank interval with an ordinary least-squares line.",
    latex: String.raw`\ln f(r)=b+m\ln r`,
    detail: "The optional trend uses unweighted OLS on natural-log raw frequency and rank. Ranks 1–3 and frequency-one types remain visible but are excluded from the regression. The empirical curve is the default; the eligible interval is an implementation convention without a documented statistical justification.",
    symbols: [
      { sym: String.raw`r`, def: "Frequency rank (1 = most frequent)." },
      { sym: String.raw`f(r)`, def: "Raw frequency at rank r." },
      { sym: String.raw`m`, def: "Log–log slope over the eligible interval." },
      { sym: String.raw`b`, def: "Intercept on the natural-log raw-frequency scale." },
      { sym: String.raw`R^2`, def: "1 − residual squared error / total squared log-frequency deviation; unavailable when total response variance is effectively zero." },
    ],
    interpretation: "Read observed points first. A slope and R² describe the selected log–log interval; they do not establish a power-law distribution.",
    limitations: [
      "No power-law goodness-of-fit test, uncertainty interval or tokenisation validation is supplied.",
      "The fixed rank and frequency exclusions have no documented statistical justification.",
      "Per-million scaling changes the intercept, while slope and R² remain unchanged.",
    ],
  },
  heapsFit: {
    id: "heaps-fit",
    label: "Observed-order vocabulary growth",
    explanation:
      "Describes how the number of distinct types grows as more tokens are read.",
    latex: String.raw`V(N)\approx K\,N^{\beta}\;\Longleftrightarrow\;\ln V\approx\ln K+\beta\ln N`,
    detail: "Documents accumulate in stable source-path order, preserving within-document token order. Distinct types are recorded at power-of-two token checkpoints plus the final total. Unweighted natural-log OLS uses checkpoints with N > 1 and V > 1; beta is unconstrained.",
    symbols: [
      { sym: String.raw`N`, def: "Tokens read so far." },
      { sym: String.raw`V(N)`, def: "Distinct types observed after N tokens." },
      { sym: String.raw`K`, def: "Fitted coefficient - the scale of the growth curve." },
      { sym: String.raw`\beta`, def: "Log-log slope along this accumulation path." },
      { sym: String.raw`R^2`, def: "Descriptive regression summary of the nested log-log checkpoints." },
    ],
    interpretation: "A smaller beta describes slower fitted growth over the observed range in this order. Different document orders can produce different trajectories. Neither beta nor R-squared establishes saturation, representativeness or a vocabulary ceiling.",
    limitations: [
      "The nested checkpoints describe one observed accumulation path, not an order-invariant corpus property.",
      "No document-order permutations, uncertainty bands or extrapolated vocabulary ceiling are computed.",
    ],
  },
  topNCoverage: {
    id: "top-n-coverage",
    label: "Top-N coverage",
    explanation: "The share of all tokens accounted for by the N most frequent types.",
    latex: String.raw`C_N=\frac{1}{T}\sum_{i=1}^{N}f_{(i)}`,
    detail: "Sort types by frequency, take the top N and divide their combined token count by the corpus total. This describes the share of counted tokens carried by those surface types, not a reader's comprehension.",
    symbols: [
      { sym: String.raw`C_N`, def: "Coverage of the top N types, in [0, 1]." },
      { sym: String.raw`f_{(i)}`, def: "Frequency of the i-th most frequent type." },
      { sym: String.raw`T`, def: "Total tokens in the corpus." },
    ],
    interpretation: "Read coverage at a stated N with the observed frequency distribution. A small number of types can carry much of the token mass; this does not establish semantic coverage or a quality category.",
    limitations: [
      "Depends on tokenisation, NFC normalisation and the lowercased aggregation keys, like every frequency measure.",
    ],
  },
  normalisedEntropy: {
    id: "normalised-entropy",
    label: "Normalised entropy",
    explanation: "Bounds entropy relative to the maximum for the observed type count; equal frequencies give 1.",
    latex: String.raw`H_{\mathrm{norm}}=\frac{H}{\log_2 V},\qquad H=-\sum_{i=1}^{V}p_i\log_2 p_i`,
    detail: "Divide empirical H by log2(V), the maximum for the observed support. This bounds the ratio relative to that maximum; it does not remove differences in corpus size, composition, sampling, tokenisation, processing or estimator behaviour.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (base-2 logarithm)." },
      { sym: String.raw`p_i`, def: "Relative frequency of type i - its token count divided by the total tokens N." },
      { sym: String.raw`V`, def: "Number of distinct types (vocabulary size)." },
      { sym: String.raw`\log_2 V`, def: "Maximum possible entropy, reached when all V types are equally frequent." },
      { sym: String.raw`H_{\mathrm{norm}}`, def: "Normalised entropy - H divided by its maximum, in [0, 1]." },
    ],
    interpretation: "A higher ratio means a more even observed frequency distribution relative to the maximum for V. Comparisons remain descriptive and conditional on corpus design and processing.",
    limitations: [
      "No finite-sample bias correction or uncertainty interval is applied; identical processing alone does not establish comparability.",
      "Unavailable for an empty or single-type corpus, where log2(V) cannot provide the denominator.",
    ],
  },
  effectiveVocabulary: {
    id: "effective-vocabulary",
    label: "Entropy-equivalent vocabulary size",
    explanation: "Gives the number of equally frequent types that would have the same entropy as this corpus.",
    latex: String.raw`V_{\mathrm{eff}}=2^H`,
    detail:
      "This is the count of equally frequent types with the same empirical entropy (the effective-number transformation in Hill, 1973, expressed for base-2 entropy). It describes the observed frequency distribution; it does not estimate unseen types.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (the same H as in normalised entropy)." },
      { sym: String.raw`2^H`, def: "Two raised to H, because H is measured in bits (base-2)." },
      { sym: String.raw`V_{\mathrm{eff}}`, def: "Entropy-equivalent vocabulary size - the equivalent count of equally frequent types." },
    ],
    interpretation: "Compare this equivalent count with the observed type count. A gap records frequency inequality; it does not establish a Zipf distribution or describe unseen vocabulary. Perplexity here is this empirical quantity, not a language-model evaluation.",
    limitations: [
      "Inherits H's dependence on the full observed distribution, sampling and type definition.",
      "No bias correction or uncertainty interval is applied.",
    ],
  },
  griesDp: {
    id: "gries-dp",
    label: "Gries DP",
    explanation: "Compares a type’s distribution across documents with the documents’ shares of corpus tokens.",
    latex: String.raw`\mathrm{DP}=\frac{1}{2}\sum_{d=1}^{D}\lvert p_d-s_d\rvert`,
    detail:
      "For a given type, DP measures how far its actual spread across documents departs from what you would expect if it were distributed in proportion to document sizes. For each document, take the absolute difference between the type's observed document share and that document's share of all corpus tokens; sum these differences and halve.",
    symbols: [
      { sym: String.raw`D`, def: "Number of documents in the corpus." },
      {
        sym: String.raw`p_d`,
        def: "Observed proportion of this type's tokens that occur in document d (these sum to 1 across documents).",
      },
      {
        sym: String.raw`s_d`,
        def: "Document d's size as a fraction of total corpus tokens - its expected share (these sum to 1).",
      },
      {
        sym: String.raw`\lvert p_d-s_d\rvert`,
        def: "Absolute deviation between observed and expected for document d.",
      },
      {
        sym: String.raw`\frac{1}{2}\sum`,
        def: "Half the summed deviations, which bounds DP to [0, 1].",
      },
    ],
    interpretation: "DP = 0 matches the size-proportional reference; larger DP means greater departure. DP does not identify a cause such as topic, register or sampling composition. The Overview reports a median for types with at least 10 occurrences; that reporting selection is not an adequacy threshold.",
    limitations: [
      "Reports Gries's original, unnormalised DP. Its maximum is 1 minus the smallest positive document token share; CorpusWright does not report DPnorm.",
      "Raw DP values under different document partitions can have different maxima. Low-frequency occurrence allocations are coarse; read DP with count and document range. A size-proportional reference is not a fitted sampling model.",
    ],
  },
  documentFrequencySpectrum: {
    id: "document-frequency-spectrum",
    label: "Document-frequency spectrum",
    explanation: "Groups types by the number of documents in which each type occurs.",
    latex: String.raw`\mathrm{df}(w)=\left\lvert\{d:f_d(w)>0\}\right\rvert`,
    detail:
      "Groups the vocabulary by document frequency - the number of documents in which each type appears at least once. Read this reach alongside raw token count; the two measures answer different questions about the observed occurrences.",
    symbols: [
      { sym: String.raw`\mathrm{df}(w)`, def: "Document frequency of type w - the count of documents containing it." },
      { sym: String.raw`w`, def: "A type in the vocabulary." },
      { sym: String.raw`d`, def: "A document in the corpus." },
      { sym: String.raw`f_d(w)`, def: "Frequency of type w within document d." },
      {
        sym: String.raw`\lvert\{\ldots\}\rvert`,
        def: "The number of documents whose within-document frequency for w is greater than zero.",
      },
    ],
    interpretation: "High token frequency with low document frequency means many occurrences in few documents. Document frequency records reach without establishing topic specificity or use outside this sample. The ≥90% callout is a fixed coverage reference, not a natural vocabulary category.",
    limitations: [
      "Depends on how the corpus is partitioned into documents; merging or splitting files can change values even when the text is unchanged.",
      "Counts in how many documents a type occurs, not how often within each.",
    ],
  },
} as const satisfies Record<string, OverviewGlossaryEntry>;

export type OverviewGlossaryId = keyof typeof overviewGlossary;
