"""Generate Method and Measures figures for the static documentation site.

The toy corpus below is a constructed, idealised example. It is used only to
show how a measure responds to structure. Distribution-shape figures use a
disclosed interim public-domain corpus until the real demo corpus is chosen.
"""

from __future__ import annotations

import argparse
import re
from collections import Counter
from pathlib import Path

import matplotlib

matplotlib.use("Agg")  # non-interactive, file-only backend

import matplotlib.pyplot as plt
from matplotlib import font_manager
import numpy as np
import pandas as pd


# Vendored IBM Plex Sans TTFs (OFL) -- registered so the SVGs bake Plex
# outlines instead of DejaVu.
FONT_DIR = Path("docs/assets/figure-src/fonts")
for _font in sorted(FONT_DIR.glob("*.ttf")):
    font_manager.fontManager.addfont(str(_font))
plt.rcParams["font.family"] = "IBM Plex Sans"
plt.rcParams["svg.hashsalt"] = "corpuswright-method-and-measures"

FIGURE_DIR = Path("docs/assets/figures")
CORPUS_PATH = Path("docs/assets/figure-src/data/the_man_who_was_thursday.txt")

CAPTIONS = {
    "workflow_loop": "Preparation is analysis: saved cleaning rules replay deterministically into each rebuild.",
    "eda_chain": "A chain of diagnostic questions that cuts across measures, from what is in the corpus to what can be asked next.",
    "frequency": "Same raw count of 120 — 500 per million in a small corpus, 10 per million in a large one.",
    "dispersion": "Same frequency, opposite corpus behaviour — balanced four-document toy corpus, each document 25% of tokens.",
    "ttr_curve": "TTR falls as text length grows, because longer texts necessarily repeat common words. Computed from the full text of Chesterton's The Man Who Was Thursday (1908, Project Gutenberg; lowercased alphabetic tokens) — an interim, fully disclosed stand-in for the demo corpus.",
    "hapax_spectrum": "Most types are rare — a long tail of hapaxes and dislegomena dominates the type inventory. Occurrence counts 1–10 shown. Computed from the full text of Chesterton's The Man Who Was Thursday (1908, Project Gutenberg; lowercased alphabetic tokens) — an interim, fully disclosed stand-in for the demo corpus.",
    "zipf": "Rank-frequency on log-log axes — a straight-ish line of slope −a, with the head and frequency-one tail excluded from the fit; the R² describes the fitted line, not proof the law holds. Computed from the full text of Chesterton's The Man Who Was Thursday (1908, Project Gutenberg; lowercased alphabetic tokens) — an interim, fully disclosed stand-in for the demo corpus.",
    "heaps": "New types keep arriving but the rate of discovery slows — a concave vocabulary growth curve. Computed from the full text of Chesterton's The Man Who Was Thursday (1908, Project Gutenberg; lowercased alphabetic tokens) — an interim, fully disclosed stand-in for the demo corpus.",
    "entropy": "Entropy measures how evenly probability is shared — even four-type distribution vs one type carrying most of the mass.",
    "effective_documents": "Four documents on paper — but one large document can make the corpus behave like fewer.",
    "reading_order": "A suggested reading order across the overview diagnostics, looping back to cleaning when something looks wrong.",
}


def style_axes(ax):
    """Shared figure style: light grid behind the data, top/right spines off."""
    ax.grid(True, alpha=0.4, linewidth=0.4, color="grey")
    ax.set_axisbelow(True)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    return ax


def new_axes(nrows=1, ncols=1, **kwargs):
    """subplots() that pre-styles every axis, so figure cells stay short."""
    kwargs.setdefault("layout", "constrained")
    fig, axes = plt.subplots(nrows, ncols, **kwargs)
    for ax in np.atleast_1d(axes).ravel():
        style_axes(ax)
    return fig, axes


def save_figure(fig, name):
    """Write FIGURE_DIR/<name>.svg: transparent, tight bbox, then close."""
    FIGURE_DIR.mkdir(parents=True, exist_ok=True)
    fig.savefig(
        FIGURE_DIR / f"{name}.svg",
        format="svg",
        bbox_inches="tight",
        metadata={"Date": None},
        transparent=True,
    )
    plt.close(fig)


def load_corpus():
    """TIER 3 token stream for the shape figures (list[str]).

    Interim source (2 Jul): the full text of Chesterton's The Man Who Was
    Thursday (1908, Project Gutenberg #1695), vendored with the Gutenberg
    boilerplate stripped. Tokens are lowercased alphabetic strings -- a
    deliberately simple rule, disclosed in every caption that uses it.
    Swap for the real demo corpus when it lands, and relabel the captions.
    """
    text = CORPUS_PATH.read_text(encoding="utf-8")
    return re.findall(r"[a-z]+", text.lower())


# Balanced corpus: four documents, 1,000 tokens each, 4,000 total.
DOC_TOKENS_BALANCED = pd.Series(
    {"Doc 1": 1000, "Doc 2": 1000, "Doc 3": 1000, "Doc 4": 1000}
)

# Dominated variant: same four docs, same 4,000 total, skewed sizes.
DOC_TOKENS_DOMINATED = pd.Series(
    {"Doc 1": 2000, "Doc 2": 667, "Doc 3": 667, "Doc 4": 666}
)

# Fixed numeric skeleton -- never changes across themes.
# Roles: spread (DP=0), concentrated (DP=0.75), function word, hapax (x1).
ROLE_COUNTS = {
    "spread": [3, 3, 3, 3],
    "concentrated": [12, 0, 0, 0],
    "function": [80, 76, 83, 79],
    "hapax": [1, 0, 0, 0],
}

# Interchangeable vocabulary skins -- same arithmetic, different words.
THEMES = {
    "borges": {
        "spread": "labyrinth",
        "concentrated": "tiger",
        "function": "the",
        "hapax": "codex",
    },
    "orwell": {
        "spread": "Party",
        "concentrated": "Newspeak",
        "function": "the",
        "hapax": "paperweight",
    },
    "corpus_linguistics": {
        "spread": "corpus",
        "concentrated": "dispersion",
        "function": "the",
        "hapax": "hapax",
    },
}

# Pick one theme per page; swap this line for a fresh example elsewhere.
ACTIVE_THEME = "borges"


def toy_words(theme=ACTIVE_THEME):
    """Per-document word counts for a chosen vocabulary theme."""
    words = THEMES[theme]
    return pd.DataFrame(
        {words[role]: counts for role, counts in ROLE_COUNTS.items()},
        index=["Doc 1", "Doc 2", "Doc 3", "Doc 4"],
    )


TOY_WORDS = toy_words()
WORDS = THEMES[ACTIVE_THEME]


def normalised_frequency(raw, total, base):
    """Rescale a raw count to occurrences per `base` tokens."""
    return raw / total * base


def deviation_of_proportions(counts, sizes):
    """Gries's unnormalised DP: 0 = spread as sizes predict, high = clumped."""
    return 0.5 * np.abs(counts / counts.sum() - sizes / sizes.sum()).sum()


def shannon_entropy(probabilities):
    """Shannon entropy in bits; assumes the probabilities sum to 1."""
    p = np.asarray(probabilities, dtype=float)
    p = p[p > 0]
    return float(-(p * np.log2(p)).sum())


def normalised_entropy(probabilities):
    """Shannon entropy divided by its maximum, log2(V); lands in [0, 1]."""
    v = np.count_nonzero(probabilities)
    return shannon_entropy(probabilities) / np.log2(v)


def effective_document_count(sizes):
    """1 / Σ sᵢ² — equally sized documents the corpus behaves like."""
    shares = sizes / sizes.sum()
    return float(1.0 / np.square(shares).sum())


def ttr_curve(tokens, step=100):
    """Running TTR at growing prefixes of a token stream."""
    seen, checkpoints, ratios = set(), [], []
    for i, token in enumerate(tokens, start=1):
        seen.add(token)
        if i % step == 0:
            checkpoints.append(i)
            ratios.append(len(seen) / i)
    return np.array(checkpoints), np.array(ratios)


def frequency_spectrum(tokens):
    """Frequency of frequencies: {times_a_type_occurs: number_of_types}."""
    return Counter(Counter(tokens).values())


def zipf_data(tokens):
    """Ranks (1..V) and frequencies sorted high-to-low."""
    freqs = np.array(sorted(Counter(tokens).values(), reverse=True))
    return np.arange(1, freqs.size + 1), freqs


def zipf_fit(ranks, freqs):
    """OLS of log f on log r, excluding rank 1 and the frequency-one tail."""
    keep = (ranks > 1) & (freqs > 1)
    log_r, log_f = np.log10(ranks[keep]), np.log10(freqs[keep])
    slope, intercept = np.polyfit(log_r, log_f, deg=1)
    predicted = slope * log_r + intercept
    ss_res = np.sum((log_f - predicted) ** 2)
    ss_tot = np.sum((log_f - log_f.mean()) ** 2)
    return slope, intercept, 1 - ss_res / ss_tot  # exponent a = -slope


def heaps_curve(tokens, step=100):
    """Distinct types seen at growing token checkpoints, in reading order."""
    seen, checkpoints, vocab = set(), [], []
    for i, token in enumerate(tokens, start=1):
        seen.add(token)
        if i % step == 0:
            checkpoints.append(i)
            vocab.append(len(seen))
    return np.array(checkpoints), np.array(vocab)


def heaps_fit(n_tokens, vocab):
    """Fit V = K * N**beta by OLS on log-log axes."""
    beta, log_k = np.polyfit(np.log10(n_tokens), np.log10(vocab), deg=1)
    return 10**log_k, beta


def make_frequency():
    freq = pd.DataFrame(
        {
            "corpus": ["Small", "Large"],
            "raw": [120, 120],
            "total": [240_000, 12_000_000],
        }
    )
    freq["pmw"] = normalised_frequency(freq["raw"], freq["total"], 1_000_000)  # 500, 10

    fig, ax = new_axes(figsize=(5, 2.4))
    ax.barh(freq["corpus"], freq["pmw"], color="0.3")
    ax.invert_yaxis()  # Small on top -- the prose meets the small corpus first
    ax.set(xlabel="per million words")  # no in-figure title: the figcaption carries it
    for y, v in enumerate(freq["pmw"]):
        ax.text(v, y, f"  {v:.0f} pmw", va="center")
    save_figure(fig, "frequency")


def make_dispersion():
    sizes = DOC_TOKENS_BALANCED.to_numpy()
    pair = [WORDS["spread"], WORDS["concentrated"]]  # both total 12, opposite DP

    fig, axes = new_axes(2, 1, figsize=(5, 3.2), sharex=True)
    for ax, word in zip(axes, pair):
        counts = TOY_WORDS[word].to_numpy()
        dp = deviation_of_proportions(counts, sizes)
        ax.bar(TOY_WORDS.index, counts, color="0.3")
        # Explicit zeros: absence is a value, not missing data.
        for x, c in enumerate(counts):
            ax.text(x, c + 0.4, str(c), ha="center", fontsize=9)
        ax.set(ylim=(0, 14), title=f"{word}: 12 occurrences, DP = {dp:.2f}")
    save_figure(fig, "dispersion")  # no suptitle: the figcaption carries it


def make_entropy():
    dists = {"even": [0.25, 0.25, 0.25, 0.25], "skewed": [0.7, 0.1, 0.1, 0.1]}

    fig, axes = new_axes(1, 2, figsize=(6, 2.8), sharey=True)
    for ax, (name, p) in zip(axes, dists.items()):
        ax.bar(list("ABCD"), p, color="0.3")  # abstract labels by design
        h = shannon_entropy(p)  # 2.00/1.36 bits
        h_norm = normalised_entropy(p)  # 1.00/0.68
        ax.set_title(f"{name}\nH = {h:.2f} bits, H_norm = {h_norm:.2f}")
    axes[0].set_ylabel("probability")
    save_figure(fig, "entropy")


def make_effective_documents():
    corpora = {"Balanced": DOC_TOKENS_BALANCED, "Dominated": DOC_TOKENS_DOMINATED}

    fig, axes = new_axes(2, 1, figsize=(5, 3.2), sharex=True)  # SHARED x-axis --
    # without it the balanced bars render as long as the dominated Doc 1: misleading
    for ax, (name, sizes) in zip(axes, corpora.items()):
        eff = effective_document_count(sizes.to_numpy())  # 4.0 then ~3.0
        ax.barh(sizes.index, sizes.to_numpy(), color="0.3")
        ax.invert_yaxis()
        for y, v in enumerate(sizes.to_numpy()):
            ax.text(v, y, f" {v:,}", va="center", fontsize=9)
        ax.set_title(f"{name}: effective documents = {eff:.1f}")
    axes[0].set_xlim(0, 2300)  # headroom for the count labels
    axes[1].set_xlabel("tokens")
    save_figure(fig, "effective_documents")


def make_ttr_curve():
    tokens = load_corpus()  # interim: Chesterton -- disclosed in the caption
    n, ttr = ttr_curve(tokens)

    fig, ax = new_axes(figsize=(5, 2.6))
    ax.plot(n, ttr, color="0.3")
    ax.set(xlabel="tokens read", ylabel="TTR")  # no title: the figcaption carries it
    save_figure(fig, "ttr_curve")


def make_hapax_spectrum():
    tokens = load_corpus()  # interim: Chesterton -- disclosed in the caption
    spectrum = frequency_spectrum(tokens)  # {times_occurring: number_of_types}
    occ = range(1, 11)  # head of the spectrum only: on real text the tail runs
    # to the thousands ("the") and would crush the hapax/dislegomena story

    fig, ax = new_axes(figsize=(5, 2.6))
    ax.bar([str(k) for k in occ], [spectrum.get(k, 0) for k in occ], color="0.3")
    ax.set(xlabel="times a type occurs", ylabel="number of types")
    save_figure(fig, "hapax_spectrum")  # no title: the figcaption carries it


def make_zipf():
    tokens = load_corpus()  # interim: Chesterton -- disclosed in the caption
    ranks, freqs = zipf_data(tokens)
    slope, intercept, r2 = zipf_fit(ranks, freqs)

    fig, ax = new_axes(figsize=(5, 4))
    ax.loglog(ranks, freqs, ".", color="0.3")
    ax.set(
        xlabel="rank",
        ylabel="frequency",
        title=f"Zipf: a = {-slope:.2f}, R² = {r2:.2f}",
    )  # title carries data: stays
    save_figure(fig, "zipf")


def make_heaps():
    tokens = load_corpus()  # interim: Chesterton -- disclosed in the caption
    n, vocab = heaps_curve(tokens)
    k, beta = heaps_fit(n, vocab)

    fig, ax = new_axes(figsize=(5, 4))
    ax.plot(n, vocab, color="0.3")
    ax.set(xlabel="tokens", ylabel="types", title=f"Heaps: β = {beta:.2f}")
    save_figure(fig, "heaps")


READY_FIGURES = {
    "frequency": make_frequency,
    "dispersion": make_dispersion,
    "entropy": make_entropy,
    "effective_documents": make_effective_documents,
    "ttr_curve": make_ttr_curve,
    "hapax_spectrum": make_hapax_spectrum,
    "zipf": make_zipf,
    "heaps": make_heaps,
}


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "figures",
        nargs="*",
        default=list(READY_FIGURES),
        choices=sorted(READY_FIGURES),
        help="Figure keys to generate. Defaults to all figures.",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    for name in args.figures:
        READY_FIGURES[name]()


if __name__ == "__main__":
    main()
