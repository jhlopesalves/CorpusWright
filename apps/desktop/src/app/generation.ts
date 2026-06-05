declare const generationDomain: unique symbol;

/** Opaque lifecycle token; domains cannot be assigned to one another in typed code. */
export type Generation<Domain extends string> = number & {
  readonly [generationDomain]: Domain;
};

export class GenerationCounter<Domain extends string> {
  #value = 0;

  next(): Generation<Domain> {
    this.#value += 1;
    return this.#value as Generation<Domain>;
  }

  invalidate(): void {
    this.#value += 1;
  }

  isCurrent(generation: Generation<Domain>): boolean {
    return generation === this.#value;
  }
}
