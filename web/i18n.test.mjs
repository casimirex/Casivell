// Tests for the string tables, run with `node web/i18n.test.mjs`.
//
// Two things worth asserting mechanically. Language selection has real branching — a stored
// choice, then the browser's list, then a fallback — and a wrong fallback would silently show
// the wrong language to everyone. And the two tables must carry the same keys: a missing one
// renders as `undefined` on the page, or as nothing at all.

import assert from "node:assert/strict";
import { MESSAGES, TERMS, initialLanguage } from "./i18n.js";

let failures = 0;
function test(name, run) {
  try {
    run();
    console.log(`  ok  ${name}`);
  } catch (error) {
    failures += 1;
    console.error(`  FAIL ${name}\n       ${error.message}`);
  }
}

console.log("i18n");

test("both tables carry exactly the same keys", () => {
  const de = Object.keys(MESSAGES.de).sort();
  const en = Object.keys(MESSAGES.en).sort();
  const missing = de.filter(key => !en.includes(key));
  const extra = en.filter(key => !de.includes(key));
  assert.deepEqual(missing, [], "keys present in de but not en would render empty");
  assert.deepEqual(extra, [], "keys present in en but not de");
});

test("every value is a string or a function, never undefined", () => {
  for (const [language, table] of Object.entries(MESSAGES)) {
    for (const [key, value] of Object.entries(table)) {
      const kind = typeof value;
      assert.ok(
        kind === "string" || kind === "function" || kind === "object",
        `${language}.${key} is ${kind}`
      );
      if (kind === "string") assert.ok(value.length > 0, `${language}.${key} is empty`);
    }
  }
});

test("the message functions take the same arity in both languages", () => {
  for (const key of Object.keys(MESSAGES.de)) {
    if (typeof MESSAGES.de[key] !== "function") continue;
    assert.equal(typeof MESSAGES.en[key], "function", `${key} is a function only in German`);
    assert.equal(
      MESSAGES.en[key].length, MESSAGES.de[key].length,
      `${key} takes different arguments in the two languages`
    );
  }
});

test("the error tables cover the same codes", () => {
  assert.deepEqual(
    Object.keys(MESSAGES.de.errors).sort(),
    Object.keys(MESSAGES.en.errors).sort()
  );
});

test("statutory terms are German and shared, not translated", () => {
  // The point of the design: these are the words on the payslip, and there is one set of them.
  assert.equal(TERMS.incomeTax, "Lohnsteuer");
  assert.equal(TERMS.net, "Nettolohn");
  for (const table of Object.values(MESSAGES)) {
    assert.ok(!("incomeTax" in table), "a statutory term leaked into a language table");
  }
});

test("a stored choice wins", () => {
  assert.equal(initialLanguage("en", ["de-DE"]), "en");
  assert.equal(initialLanguage("de", ["en-GB"]), "de");
});

test("an unknown stored choice falls through to the browser", () => {
  assert.equal(initialLanguage("fr", ["en-GB", "de"]), "en");
});

test("the browser's list is taken in order, by base tag", () => {
  assert.equal(initialLanguage(null, ["en-GB", "de-DE"]), "en");
  assert.equal(initialLanguage(null, ["de-AT"]), "de");
  assert.equal(initialLanguage(null, ["fr-FR", "de-CH"]), "de", "skips languages we lack");
});

test("German is the fallback, not English", () => {
  // Deliberate: the figures, the terms and the documents they are compared against are German.
  assert.equal(initialLanguage(null, []), "de");
  assert.equal(initialLanguage(null, ["fr-FR"]), "de");
  assert.equal(initialLanguage(undefined, undefined), "de");
});

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
