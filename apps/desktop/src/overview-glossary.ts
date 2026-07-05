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
    interpretation:
      "Tokens measure corpus size - how much running text there is. Every 'per million' rate and every proportion on this page uses N as its denominator.",
    limitations: [
      "Counts depend on the tokenisation rules above; corpora processed with different rules are not comparable.",
      "Numbers count as tokens, so number-heavy material (tables, OCR output) inflates N.",
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
      "Types are case-folded surface forms, not lemmas - inflected variants count separately.",
      "OCR errors and spelling variants each create their own type, inflating V in noisy corpora.",
    ],
  },
  documentUnit: {
    id: "document",
    label: "Document",
    explanation:
      "One source text in the analysis corpus - the unit behind balance, dispersion, and document-frequency measures.",
    latex: String.raw`s_d=\frac{N_d}{N}`,
    detail:
      "Every file included in the analysis is one document. Balance metrics compare document sizes; dispersion metrics ask how each type spreads across documents. Documents that yield no tokens are excluded from these calculations.",
    symbols: [
      { sym: String.raw`s_d`, def: "Document d's share of all corpus tokens." },
    ],
    interpretation:
      "Every document-based measure inherits your segmentation choices. One enormous file among thousands of small ones dominates the shares - check the largest-document share in the balance panel before reading dispersion.",
    limitations: [
      "Merging or splitting source files changes every document-based measure without changing a word of text.",
    ],
  },
  giniCoefficient: {
    id: "gini-coefficient",
    label: "Gini coefficient",
    explanation: "Summarises inequality in document lengths: 0 means equal lengths, while larger values mean greater imbalance.",
    latex: String.raw`G=\frac{\sum_{i=1}^{n}\sum_{j=1}^{n}\lvert x_i-x_j\rvert}{2n\sum_{i=1}^{n}x_i}`,
    detail:
      "Borrowed from economics, the Gini coefficient summarises inequality in document lengths. It is the average absolute difference in length between all document pairs, normalised by the mean: 0 means every document is the same length, values approaching 1 mean a few documents hold most of the tokens.",
    symbols: [
      { sym: String.raw`G`, def: "Gini coefficient for document lengths." },
      { sym: String.raw`x_i,\ x_j`, def: "Token lengths of documents i and j." },
      { sym: String.raw`n`, def: "Number of documents." },
      { sym: String.raw`\sum x_i`, def: "Total tokens across all documents." },
    ],
    interpretation:
      "Runs from 0 (perfect equality) to near 1 (extreme imbalance). It tells you whether corpus-level frequencies rest on a balanced set of documents or are dominated by a few large ones. A high value is a warning that aggregate counts may really reflect a handful of documents - read it with dispersion (Gries DP) and effective documents, not on its own.",
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
    interpretation:
      "0 means all documents are the same length; higher means greater relative spread. As a rough guide, below about 0.3 is fairly uniform, 0.5-1 is moderately uneven, and above 1 means lengths vary wildly. Like Gini, it is context for reading corpus-level counts, not a quality score.",
    limitations: [
      "Assumes the mean is a meaningful centre; for very skewed length distributions the Gini view is more informative.",
      "Measures length only, not content.",
    ],
  },
  effectiveDocuments: {
    id: "effective-documents",
    label: "Effective documents",
    explanation: "Expresses document balance as the equivalent number of equally sized documents.",
    latex: String.raw`N_{\mathrm{eff}}=\frac{1}{\sum_{d=1}^{D}s_d^2}`,
    detail:
      "The inverse Simpson index applied to document size shares - the equivalent number of equally sized documents that would give the same balance as your actual corpus. When a few documents dominate, the effective count falls well below the real number.",
    symbols: [
      { sym: String.raw`N_{\mathrm{eff}}`, def: "Effective document count." },
      { sym: String.raw`s_d`, def: "Document d's share of total corpus tokens (its size as a fraction)." },
      { sym: String.raw`D`, def: "Number of documents." },
      { sym: String.raw`\sum s_d^2`, def: "Sum of squared shares - the Simpson concentration." },
    ],
    interpretation:
      "It answers: given this imbalance, how many equally sized documents would the corpus behave like? Compare it to the real document count. Close to the actual number means well balanced; much lower means a few large documents carry most of the mass. For example, '7.0 of 8' means the eight documents behave almost like eight equal ones - well balanced. It is the document-level analogue of effective vocabulary.",
    limitations: [
      "Measures balance of document sizes, not content or quality.",
      "Depends on how the corpus is segmented into documents.",
    ],
  },
  hapax: {
    id: "hapax",
    label: "Hapax",
    explanation: "A type that occurs exactly once in the analysis corpus.",
    latex: String.raw`f(w)=1`,
    detail:
      "Hapax legomena are types that occur exactly once in the analysis corpus. Their count and proportion are a quick gauge of vocabulary richness and of the long tail.",
    symbols: [
      { sym: String.raw`f(w)=1`, def: "Type w occurs exactly once." },
    ],
    interpretation:
      "In natural language a large share of types - often roughly 40-60% - are hapax; the proportion rises with vocabulary richness and falls as a corpus grows and words recur. A very high hapax rate can also flag noise (OCR errors, unnormalised variants) rather than genuine richness, so read it next to the cleaning results.",
    limitations: [
      "Sensitive to tokenisation, casing and cleaning - OCR and spelling variants inflate the count.",
      "Corpus-size-dependent; only compare across corpora of similar size.",
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
    interpretation:
      "Like hapax, a high proportion points to a rich or noisy long tail; tracking hapax and dis legomena together shows how quickly the tail thins. Both shrink as a corpus grows and types recur.",
    limitations: [
      "Sensitive to tokenisation, casing and cleaning, like hapax.",
      "Corpus-size-dependent; only compare across corpora of similar size.",
    ],
  },
  mattr: {
    id: "mattr",
    label: "MATTR",
    explanation: "Averages type–token ratios across overlapping same-size token windows within each document.",
    latex: String.raw`\mathrm{MATTR}_W=\frac{1}{N-W+1}\sum_{j=1}^{N-W+1}\frac{V_j}{W}`,
    detail:
      "Moving-average TTR. It slides a window of W tokens across the text, computes the type-token ratio inside each window, and averages them. Because every window uses the same W, it removes raw TTR's dependence on total length.",
    symbols: [
      { sym: String.raw`\mathrm{MATTR}_W`, def: "Moving-average TTR computed with window size W." },
      { sym: String.raw`W`, def: "Window size, in tokens." },
      { sym: String.raw`N`, def: "Number of tokens in the document." },
      { sym: String.raw`V_j`, def: "Number of distinct types in the window starting at position j." },
      { sym: String.raw`N-W+1`, def: "Number of overlapping windows." },
    ],
    interpretation:
      "Runs from 0 to 1; higher means richer vocabulary within a typical window. Because all windows share one size, MATTR is comparable across documents of different lengths, unlike raw TTR. Values depend on the chosen window, so only compare figures computed with the same W.",
    limitations: [
      "Depends on the window size W; figures from different windows are not comparable.",
      "Requires documents at least as long as the window.",
      "Sensitive to tokenisation and casing.",
    ],
  },
  mtld: {
    id: "mtld",
    label: "MTLD",
    explanation: "Estimates the mean length of token runs that keep their running TTR above a threshold.",
    latex: String.raw`\mathrm{MTLD}=\frac{N}{F+F_{\mathrm{partial}}}`,
    detail:
      "MTLD walks through the tokens accumulating a running type-token ratio. Each time the running TTR falls to the threshold (0.720 by convention), it closes a factor and resets. MTLD is the total tokens divided by the number of factors, plus a fractional partial factor for the leftover run; it is computed forwards and backwards and the two are averaged. Intuitively, it is the average length of text you can read before lexical variety dips to the threshold.",
    symbols: [
      { sym: String.raw`N`, def: "Number of tokens." },
      { sym: String.raw`F`, def: "Number of complete factors - runs whose running TTR fell to the 0.720 threshold." },
      {
        sym: String.raw`F_{\mathrm{partial}}`,
        def: "The fractional final factor for the leftover tokens that never reached the threshold.",
      },
    ],
    interpretation:
      "Higher MTLD means greater lexical diversity - longer runs before variety drops. Unlike raw TTR it is largely length-independent, so it is comparable across documents of different lengths. A median in the dozens to low hundreds is typical for natural prose; the figure is roughly the number of tokens you read before the running type-token ratio falls to 0.720.",
    limitations: [
      "Depends on the 0.720 threshold (the McCarthy & Jarvis 2010 convention) and is unstable for very short documents (roughly under 100 tokens).",
      "The reported value is the mean of a forward and a backward pass.",
      "Sensitive to tokenisation and casing.",
    ],
  },
  shannonEntropy: {
    id: "shannon-entropy",
    label: "Shannon entropy",
    explanation:
      "The average uncertainty, in bits, of the type of a randomly drawn token.",
    latex: String.raw`H=-\sum_{i=1}^{V}p_i\log_2 p_i`,
    detail:
      "Shannon entropy treats the corpus as a probability distribution over types. Drawing one token at random, H is the average number of yes/no questions needed to identify its type. It is highest when token mass is spread evenly across many types, and falls as a few types come to dominate.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (base-2 logarithm)." },
      { sym: String.raw`p_i`, def: "Relative frequency of type i - its count divided by total tokens." },
      { sym: String.raw`V`, def: "Number of distinct types." },
    ],
    interpretation:
      "Raw H rises with both vocabulary size and evenness, so read it through its two derived figures: normalised entropy (H rescaled to 0-1) and effective vocabulary (2 to the power H). H is the quantity those two are built from.",
    limitations: [
      "Depends on tokenisation and the type definition; only comparable across identically processed corpora.",
      "Base-2 by convention here, so that effective vocabulary is two raised to H.",
    ],
  },
  zipfFit: {
    id: "zipf-fit",
    label: "Rank-frequency (Zipf) fit",
    explanation:
      "Fits a straight line to log frequency against log rank, testing how closely the vocabulary follows a power law.",
    latex: String.raw`f(r)\approx C\,r^{-a}\;\Longleftrightarrow\;\ln f(r)=\ln C-a\ln r`,
    detail:
      "Types are ranked by frequency and a least-squares line is fitted to ln(frequency) against ln(rank). Rank 1 and the frequency-one tail are excluded before fitting: the top rank is often idiosyncratic, and the tail of once-only types forms horizontal steps that distort the slope. The fitted-ranks row reports exactly which ranks the line describes.",
    symbols: [
      { sym: String.raw`r`, def: "A type's frequency rank (1 = most frequent)." },
      { sym: String.raw`f(r)`, def: "Token frequency of the type at rank r." },
      { sym: String.raw`a`, def: "Zipf exponent - the negated slope. Classic Zipf's law predicts a near 1." },
      { sym: String.raw`C`, def: "Zipf coefficient - exp(intercept), the fitted frequency at rank 1." },
      { sym: String.raw`R^2`, def: "Share of log-frequency variance explained, over the fitted ranks only." },
    ],
    interpretation:
      "The exponent says how steeply frequency falls with rank: near 1 is the classic Zipfian profile; steeper means high-frequency types dominate more; shallower means a flatter, more even vocabulary. Read the fitted range before the parameters - the fit describes only those ranks.",
    limitations: [
      "A high R-squared says the selected log-log segment is nearly straight. It does not prove Zipf's law and says nothing about corpus quality.",
      "Rank 1 and the frequency-one tail are excluded by design; the fit does not describe them.",
      "Least squares on log-log data is the pragmatic convention for power laws, not the statistically optimal estimator.",
      "The coefficient is scale-dependent: rescaling the y-axis (raw frequency vs rate per million) rescales C but leaves the exponent and R-squared unchanged.",
    ],
  },
  heapsFit: {
    id: "heaps-fit",
    label: "Vocabulary growth (Heaps) fit",
    explanation:
      "Describes how the number of distinct types grows as more tokens are read.",
    latex: String.raw`V(N)=K\,N^{\beta}\;\Longleftrightarrow\;\ln V=\ln K+\beta\ln N`,
    detail:
      "As the corpus is read in its current document order, vocabulary size is recorded at doubling token checkpoints, plus a final point for the whole corpus. Heaps' law proposes the power relation above; K and beta come from a least-squares line on the ln-ln checkpoints.",
    symbols: [
      { sym: String.raw`N`, def: "Tokens read so far." },
      { sym: String.raw`V(N)`, def: "Distinct types observed after N tokens." },
      { sym: String.raw`K`, def: "Fitted coefficient - the scale of the growth curve." },
      { sym: String.raw`\beta`, def: "Growth exponent. Natural-language corpora typically show values around 0.4-0.6." },
      { sym: String.raw`R^2`, def: "Fit quality on the log-log checkpoints." },
    ],
    interpretation:
      "Beta near 1 means nearly every stretch of new text brings new types (rich or noisy vocabulary, or a corpus still far from saturation); lower beta means the vocabulary is levelling off. The final observed point is simply the whole corpus: total tokens and total types.",
    limitations: [
      "The curve describes vocabulary growth in the corpus's current document order. It is not a prediction of growth under other orderings or samples - reordering documents changes the checkpoints and can change the fit.",
      "K and beta are correlated estimates from few checkpoints; read them as a pair describing the growth curve, not as precise constants.",
    ],
  },
  topNCoverage: {
    id: "top-n-coverage",
    label: "Top-N coverage",
    explanation: "The share of all tokens accounted for by the N most frequent types.",
    latex: String.raw`C_N=\frac{1}{T}\sum_{i=1}^{N}f_{(i)}`,
    detail:
      "Sort types by frequency, take the top N, and divide their combined token count by the corpus total. It shows how much of the running text a reader covers by knowing only the commonest words.",
    symbols: [
      { sym: String.raw`C_N`, def: "Coverage of the top N types, in [0, 1]." },
      { sym: String.raw`f_{(i)}`, def: "Frequency of the i-th most frequent type." },
      { sym: String.raw`T`, def: "Total tokens in the corpus." },
    ],
    interpretation:
      "In natural language the top 100 types - mostly function words - often cover 40-50% of all tokens. Very high coverage at small N signals strong concentration; compare corpora only at the same N.",
    limitations: [
      "Depends on tokenisation and case folding, like every frequency measure.",
    ],
  },
  normalisedEntropy: {
    id: "normalised-entropy",
    label: "Normalised entropy / evenness",
    explanation: "Measures how evenly token mass is distributed across types, from 0 (concentrated) to 1 (perfectly even).",
    latex: String.raw`J=\frac{H}{\log_2 V},\qquad H=-\sum_{i=1}^{V}p_i\log_2 p_i`,
    detail:
      "Shannon entropy H is the average number of bits needed to identify the type of a randomly drawn token; it is largest when every type is equally frequent. Dividing H by its maximum possible value rescales it to Pielou's evenness J on a 0-1 scale, so corpora with different vocabulary sizes can be compared.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (base-2 logarithm)." },
      { sym: String.raw`p_i`, def: "Relative frequency of type i - its token count divided by the total tokens N." },
      { sym: String.raw`V`, def: "Number of distinct types (vocabulary size)." },
      { sym: String.raw`\log_2 V`, def: "Maximum possible entropy, reached when all V types are equally frequent." },
      { sym: String.raw`J`, def: "Normalised entropy / evenness - H divided by its maximum, in [0, 1]." },
    ],
    interpretation:
      "J runs from 0 to 1. Near 0, a few types dominate almost all tokens (highly concentrated); near 1, token mass is spread evenly across types. Natural-language corpora usually sit in a middle band, because a handful of function words are very frequent while most types are rare. Higher means more even; lower means more concentrated.",
    limitations: [
      "Entirely dependent on tokenisation and what counts as a type (casing, lemmatisation, punctuation). Only comparable across corpora processed the same way.",
      "Computed in bits (base-2), to match the effective-vocabulary formula.",
    ],
  },
  effectiveVocabulary: {
    id: "effective-vocabulary",
    label: "Effective vocabulary (perplexity)",
    explanation: "Gives the number of equally frequent types that would have the same entropy as this corpus.",
    latex: String.raw`V_{\mathrm{eff}}=2^H`,
    detail:
      "The effective vocabulary is the exponential of the Shannon entropy (a Hill number of order 1, also called perplexity). It answers: how many equally frequent types would it take to produce this much entropy? It is a diversity figure that discounts the long tail of rare words.",
    symbols: [
      { sym: String.raw`H`, def: "Shannon entropy, in bits (the same H as in normalised entropy)." },
      { sym: String.raw`2^H`, def: "Two raised to H, because H is measured in bits (base-2)." },
      { sym: String.raw`V_{\mathrm{eff}}`, def: "Effective vocabulary - the equivalent count of equally frequent types." },
    ],
    interpretation:
      "Read it against the raw type count. A corpus can have tens of thousands of unique types but a much smaller effective vocabulary - meaning its diversity behaves as if there were only that many equally common words. A large gap between the two is the signature of a Zipfian long tail: a small set of high-frequency forms carries most of the token mass while the bulk of types are rare. Higher effective vocabulary means richer effective diversity. Some tools report this same quantity as perplexity - they are one number, not two metrics.",
    limitations: [
      "Derived from H, so it inherits the same base-2 and tokenisation dependence.",
      "Robust to the exact size of the rare tail, but not to how types are defined.",
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
    interpretation:
      "DP runs from 0 (the type is spread exactly in proportion to document sizes - maximally dispersed) to near 1 (the type is concentrated in one or a few documents - clumped). Lower DP means more evenly dispersed, more general vocabulary; higher DP means document- or topic-bound. The bench reports two readouts over frequent types: the count whose DP is at least 0.5 (how many are noticeably clumped), and the median DP (how uneven the typical frequent type is).",
    limitations: [
      "This is Gries's (2008) original DP. A normalised variant, DPnorm, divides the original DP by one minus the smallest document-size share, which helps when documents differ greatly in size or for cross-corpus comparison. CorpusWright reports the original DP, not DPnorm.",
      "Depends on how the corpus is segmented into documents, and on the frequent-types threshold.",
    ],
  },
  documentFrequencySpectrum: {
    id: "document-frequency-spectrum",
    label: "Document-frequency spectrum",
    explanation: "Groups types by the number of documents in which each type occurs.",
    latex: String.raw`\mathrm{df}(w)=\left\lvert\{d:f_d(w)>0\}\right\rvert`,
    detail:
      "Groups the vocabulary by document frequency - the number of documents in which each type appears at least once - rather than by raw token count. It separates words that are common because they are spread across many documents from words that are common because they cluster in a few.",
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
    interpretation:
      "Read it alongside the token-frequency spectrum. A type with high token frequency but low document frequency is bursty and topic-specific (frequent in a few documents); a type with high document frequency is genuinely widespread. The shape of the spectrum shows how much vocabulary is shared corpus-wide versus document-bound.",
    limitations: [
      "Depends entirely on how the corpus is segmented into documents - merging or splitting documents changes every value.",
      "Counts in how many documents a type occurs, not how often within each.",
    ],
  },
} as const satisfies Record<string, OverviewGlossaryEntry>;

export type OverviewGlossaryId = keyof typeof overviewGlossary;
