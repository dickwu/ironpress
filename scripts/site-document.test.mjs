import assert from "node:assert/strict";
import test from "node:test";

import { HtmlDocument } from "./site-document.mjs";

test("parses permissive raw-text end tags without manufacturing markup", () => {
  const document = HtmlDocument.parse(`<!doctype html>
    <html lang="en"><body>
      <script>const sample = "<h1>not a heading</h1>";</script data-check>
      <style>.sample::after { content: "Ironpress —"; }</style data-check>
      <main id="content"><h1>Real heading</h1><p>ironpress docs</p></main>
    </body></html>`);

  assert.equal(document.elements("script").length, 1);
  assert.equal(document.elements("style").length, 1);
  assert.equal(document.elements("h1").length, 1);
  assert.equal(document.elements("main").length, 1);
  assert.equal(document.proseText().trim(), "Real headingironpress docs");
});

test("keeps code examples out of prose without altering their text", () => {
  const document = HtmlDocument.parse(`<!doctype html>
    <html><body><main>
      <p>Use ironpress.</p>
      <pre><code>Ironpress — &lt;script&gt;example&lt;/script&gt;</code></pre>
    </main></body></html>`);

  assert.equal(document.proseText().trim(), "Use ironpress.");
  assert.equal(
    document.elements("code")[0].text(),
    "Ironpress — <script>example</script>",
  );
});

test("exposes decoded attributes and JSON-LD text", () => {
  const document = HtmlDocument.parse(`<!doctype html>
    <html lang="en"><head>
      <meta name="description" content="Fast &amp; deterministic">
      <link rel="canonical alternate" href="https://example.test/docs/">
      <script type="application/ld+json">{"@type":"TechArticle"}</script>
    </head><body><img alt="A &amp; B" id="logo"></body></html>`);

  assert.equal(document.rootLanguage(), "en");
  assert.equal(document.metaContent("description"), "Fast & deterministic");
  assert.equal(document.linkHref("canonical"), "https://example.test/docs/");
  assert.equal(document.elements("img")[0].attribute("alt"), "A & B");
  assert.deepEqual(document.ids(), ["logo"]);
  assert.deepEqual(document.jsonLdText(), ['{"@type":"TechArticle"}']);
});

test("preserves document order when resolving metadata and identifiers", () => {
  const document = HtmlDocument.parse(`<!doctype html>
    <html><head>
      <meta name="description" content="Primary description">
      <meta name="description" content="Later duplicate">
    </head><body><p id="first"></p><p id="second"></p></body></html>`);

  assert.equal(document.metaContent("description"), "Primary description");
  assert.deepEqual(document.ids(), ["first", "second"]);
});
