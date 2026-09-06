// src/lib/password-strength.ts

/** zxcvbn's 0–4 scale: 0 is trivially guessable, 4 resists an offline attack. */
export type StrengthScore = 0 | 1 | 2 | 3 | 4;

type Estimator = (password: string) => StrengthScore;

let estimator: Estimator | null = null;
let pending: Promise<boolean> | null = null;

/**
 * Pulls zxcvbn in on demand.
 *
 * The dictionaries are a few hundred kilobytes — worth it for an estimate that
 * catches keyboard patterns, dates and l33t-speak rather than counting
 * character classes, but not worth shipping to everyone who loads the landing
 * page. Called on first focus of a password field.
 *
 * Never rejects: a strength hint that fails to load is a missing hint, not a
 * broken form.
 */
export function loadEstimator(): Promise<boolean> {
  if (estimator) return Promise.resolve(true);

  if (!pending) {
    pending = Promise.all([import("@zxcvbn-ts/core"), import("@zxcvbn-ts/language-common")])
      .then(([core, common]) => {
        // v4 dropped the `zxcvbn` / `zxcvbnOptions` singletons for a factory,
        // which is just as well: the options live on this instance instead of
        // a module-wide global.
        const instance = new core.ZxcvbnFactory({
          dictionary: { ...common.dictionary },
          graphs: common.adjacencyGraphs,
        });

        estimator = (password) => instance.check(password).score as StrengthScore;
        return true;
      })
      .catch(() => {
        // Let a later focus try again rather than caching the failure forever.
        pending = null;
        return false;
      });
  }

  return pending;
}

/** `null` until {@link loadEstimator} has resolved. */
export function scorePassword(password: string): StrengthScore | null {
  return estimator ? estimator(password) : null;
}
