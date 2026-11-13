// src/locales/locales.test.ts
import { describe, expect, it } from "vitest";

type Messages = { [key: string]: string | Messages };

const files = import.meta.glob<Messages>("./*/*.json", { eager: true, import: "default" });

/** Messages by language, then by namespace (the file name). */
function byLanguage(): Record<string, Record<string, Messages>> {
  const languages: Record<string, Record<string, Messages>> = {};
  for (const [path, messages] of Object.entries(files)) {
    const [, language, namespace] = path.match(/^\.\/([^/]+)\/([^/]+)\.json$/) ?? [];
    languages[language] ??= {};
    languages[language][namespace] = messages;
  }
  return languages;
}

/** Every key path that leads to a message, sorted. */
function keys(messages: Messages, prefix = ""): string[] {
  return Object.entries(messages)
    .flatMap(([key, value]) =>
      typeof value === "string" ? [`${prefix}${key}`] : keys(value, `${prefix}${key}.`),
    )
    .sort();
}

const { en, fr } = byLanguage();

describe("locales", () => {
  it("have the same namespaces in English and French", () => {
    expect(Object.keys(fr).sort()).toEqual(Object.keys(en).sort());
  });

  it.each(Object.keys(en).sort())("have the same keys in English and French: %s", (namespace) => {
    expect(keys(fr[namespace] ?? {})).toEqual(keys(en[namespace]));
  });
});
