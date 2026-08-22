#!/usr/bin/env node

import { access, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { HtmlDocument } from "./site-document.mjs";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const errors = [];

const pages = [
  {
    path: "site/index.html",
    canonical: "https://gastongouron.github.io/ironpress/",
    structuredDataTypes: ["WebSite", "SoftwareSourceCode"],
  },
  {
    path: "site/guides/html-to-pdf-rust/index.html",
    canonical:
      "https://gastongouron.github.io/ironpress/guides/html-to-pdf-rust/",
  },
  {
    path: "site/get-started/index.html",
    canonical: "https://gastongouron.github.io/ironpress/get-started/",
    structuredDataTypes: ["CollectionPage", "ItemList"],
    requiredSections: ["choose-runtime", "capabilities"],
  },
  ...[
    ["rust", ["cargo add ironpress", "use ironpress::html_to_pdf;"]],
    ["cli", ["cargo install ironpress", "ironpress invoice.html invoice.pdf"]],
    ["c", ['#include "ironpress.h"', "ironpress_html_to_pdf", "ironpress_buffer_free"]],
    ["python", ["python -m pip install ironpress", "ironpress.html_to_pdf"]],
    ["ruby", ["gem install ironpress", "Ironpress.html_to_pdf"]],
    ["browser", ["npm install ironpress", 'from "ironpress";', "await init();", "converter.free();"]],
    ["node", ['from "ironpress/node";', "await init();", "converter.free();"]],
  ].map(([runtime, requiredText]) => ({
      path: `site/get-started/${runtime}/index.html`,
      canonical: `https://gastongouron.github.io/ironpress/get-started/${runtime}/`,
      structuredDataTypes: ["TechArticle"],
      requiredText,
      requiredSections: [
        "install",
        "first-pdf",
        "markdown",
        "configure",
        "resources",
        "limits",
        "next",
      ],
    }),
  ),
  {
    path: "playground/index.html",
    canonical: "https://gastongouron.github.io/ironpress/playground/",
  },
];

const titles = new Map();
const descriptions = new Map();

function report(path, message) {
  errors.push(`${path}: ${message}`);
}

function matches(content, pattern) {
  return [...content.matchAll(pattern)];
}

function containsJsonLdType(value, type) {
  if (Array.isArray(value)) {
    return value.some((item) => containsJsonLdType(item, type));
  }
  if (value === null || typeof value !== "object") return false;

  const declaredTypes = Array.isArray(value["@type"])
    ? value["@type"]
    : [value["@type"]];
  return (
    declaredTypes.includes(type) ||
    Object.values(value).some((item) => containsJsonLdType(item, type))
  );
}

async function load(path) {
  try {
    return await readFile(resolve(repositoryRoot, path), "utf8");
  } catch (error) {
    report(path, `cannot be read (${error.code ?? error.message})`);
    return null;
  }
}

async function requireFile(path) {
  try {
    await access(resolve(repositoryRoot, path));
  } catch (error) {
    report(path, `cannot be accessed (${error.code ?? error.message})`);
  }
}

for (const page of pages) {
  const html = await load(page.path);
  if (html === null) continue;
  const document = HtmlDocument.parse(html);
  const proseText = document.proseText();

  if (!/^<!doctype html>/i.test(html.trimStart())) {
    report(page.path, "must start with an HTML5 doctype");
  }
  if (document.rootLanguage()?.toLowerCase() !== "en") {
    report(page.path, 'must declare <html lang="en">');
  }
  if (
    !document
      .elements("meta")
      .some((element) => element.attribute("charset")?.toLowerCase() === "utf-8")
  ) {
    report(page.path, "must declare UTF-8");
  }
  if (document.metaContent("viewport") === undefined) {
    report(page.path, "must include a viewport meta tag");
  }

  const title = document.elements("title")[0]?.text().trim();
  if (!title) {
    report(page.path, "must include a non-empty title");
  } else if (titles.has(title)) {
    report(page.path, `duplicates the title used by ${titles.get(title)}`);
  } else {
    titles.set(title, page.path);
  }

  const description = document.metaContent("description")?.trim();
  if (!description || description.length < 70 || description.length > 170) {
    report(page.path, "meta description must contain 70 to 170 characters");
  } else if (descriptions.has(description)) {
    report(
      page.path,
      `duplicates the description used by ${descriptions.get(description)}`,
    );
  } else {
    descriptions.set(description, page.path);
  }

  const canonical = document.linkHref("canonical");
  if (canonical !== page.canonical) {
    report(page.path, `canonical URL must be ${page.canonical}`);
  }

  if (document.elements("h1").length !== 1) {
    report(page.path, "must contain exactly one h1");
  }
  if (document.elements("main").length !== 1) {
    report(page.path, "must contain exactly one main landmark");
  }
  if (/IronPress|Ironpress/.test(proseText)) {
    report(page.path, 'must use the lowercase "ironpress" brand in prose');
  }
  if (proseText.includes("—")) {
    report(page.path, "must not use em dashes in prose");
  }
  for (const image of document.elements("img")) {
    if (!image.attribute("alt")?.trim()) {
      report(page.path, "every image must have non-empty alt text");
    }
  }
  for (const link of document.elements("a")) {
    if (
      link.attribute("target")?.toLowerCase() === "_blank" &&
      !link.attributeTokens("rel").includes("noopener")
    ) {
      report(page.path, 'target="_blank" links must use rel="noopener"');
    }
  }

  for (const text of page.requiredText ?? []) {
    if (!html.includes(text)) {
      report(page.path, `must include the tested consumer contract: ${text}`);
    }
  }

  const structuredData = [];
  for (const source of document.jsonLdText()) {
    try {
      structuredData.push(JSON.parse(source));
    } catch (error) {
      report(page.path, `contains invalid JSON-LD (${error.message})`);
    }
  }
  for (const type of page.structuredDataTypes ?? []) {
    if (!structuredData.some((value) => containsJsonLdType(value, type))) {
      report(page.path, `must expose ${type} JSON-LD structured data`);
    }
  }

  const seenIds = new Set();
  for (const id of document.ids()) {
    if (seenIds.has(id)) report(page.path, `contains duplicate id="${id}"`);
    seenIds.add(id);
  }
  for (const id of page.requiredSections ?? []) {
    if (!seenIds.has(id)) {
      report(page.path, `must contain the standard #${id} section`);
    }
  }
}

const playground = await load("playground/index.html");
if (playground !== null) {
  const document = HtmlDocument.parse(playground);
  for (const id of ["mode", "examples", "input", "preview"]) {
    const hasLabel = document
      .elements("label")
      .some((label) => label.attribute("for") === id);
    const control = document
      .elements()
      .find(
        (element) =>
          ["iframe", "select", "textarea"].includes(element.tagName()) &&
          element.attribute("id") === id,
      );
    const hasAccessibleAttribute = ["aria-label", "aria-labelledby"].some(
      (name) => control?.attribute(name)?.trim(),
    );
    if (!hasLabel && !hasAccessibleAttribute) {
      report("playground/index.html", `#${id} needs an accessible name`);
    }
  }
}

const sitemap = await load("site/sitemap.xml");
if (sitemap !== null) {
  for (const page of pages) {
    if (!sitemap.includes(`<loc>${page.canonical}</loc>`)) {
      report("site/sitemap.xml", `must include ${page.canonical}`);
    }
  }
}

const robots = await load("site/robots.txt");
if (
  robots !== null &&
  !robots.includes("Sitemap: https://gastongouron.github.io/ironpress/sitemap.xml")
) {
  report("site/robots.txt", "must advertise the absolute sitemap URL");
}

const googleVerification = await load("site/google631b721eb1433979.html");
if (
  googleVerification !== null &&
  googleVerification.trim() !==
    "google-site-verification: google631b721eb1433979.html"
) {
  report(
    "site/google631b721eb1433979.html",
    "must contain the exact Google Search Console verification token",
  );
}

await load("site/assets/styles.css");
await load("site/assets/guide.css");
await load("site/assets/get-started.css");
await requireFile("site/assets/ironpress-logo.png");

const readme = await load("README.md");
if (readme !== null) {
  const badgeCount = matches(readme, /\[!\[/g).length;
  if (badgeCount < 14) {
    report("README.md", `must retain all 14 badges (found ${badgeCount})`);
  }
  for (const url of [
    "https://gastongouron.github.io/ironpress/",
    "https://gastongouron.github.io/ironpress/get-started/",
    "https://gastongouron.github.io/ironpress/playground/",
    "https://gastongouron.github.io/ironpress/guides/html-to-pdf-rust/",
  ]) {
    if (!readme.includes(url)) {
      report("README.md", `must link to ${url}`);
    }
  }
}

const bindingsReadme = await load("bindings/README.md");
if (bindingsReadme !== null) {
  for (const runtime of [
    "rust",
    "cli",
    "python",
    "ruby",
    "browser",
    "node",
  ]) {
    const url = `https://gastongouron.github.io/ironpress/get-started/${runtime}/`;
    if (!bindingsReadme.includes(url)) {
      report("bindings/README.md", `must link to ${url}`);
    }
  }
}

if (errors.length > 0) {
  console.error(`Static site contract failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  console.log(`Static site contract passed for ${pages.length} public pages.`);
}
