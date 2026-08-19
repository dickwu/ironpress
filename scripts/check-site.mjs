#!/usr/bin/env node

import { access, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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
  const documentMarkup = html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, "");

  if (!/^<!doctype html>/i.test(html.trimStart())) {
    report(page.path, "must start with an HTML5 doctype");
  }
  if (!/<html\b[^>]*\blang=["']en["']/i.test(html)) {
    report(page.path, 'must declare <html lang="en">');
  }
  if (!/<meta\b[^>]*charset=["']?utf-8/i.test(html)) {
    report(page.path, "must declare UTF-8");
  }
  if (!/<meta\b[^>]*name=["']viewport["'][^>]*>/i.test(html)) {
    report(page.path, "must include a viewport meta tag");
  }

  const title = html.match(/<title>([^<]+)<\/title>/i)?.[1]?.trim();
  if (!title) {
    report(page.path, "must include a non-empty title");
  } else if (titles.has(title)) {
    report(page.path, `duplicates the title used by ${titles.get(title)}`);
  } else {
    titles.set(title, page.path);
  }

  const description = html.match(
    /<meta\b[^>]*name=["']description["'][^>]*content=["']([^"']+)["'][^>]*>/i,
  )?.[1]?.trim();
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

  const canonical = html.match(
    /<link\b[^>]*rel=["']canonical["'][^>]*href=["']([^"']+)["'][^>]*>/i,
  )?.[1];
  if (canonical !== page.canonical) {
    report(page.path, `canonical URL must be ${page.canonical}`);
  }

  if (matches(documentMarkup, /<h1\b/gi).length !== 1) {
    report(page.path, "must contain exactly one h1");
  }
  if (matches(documentMarkup, /<main\b/gi).length !== 1) {
    report(page.path, "must contain exactly one main landmark");
  }
  for (const image of matches(documentMarkup, /<img\b[^>]*>/gi)) {
    if (!/\balt=["'][^"']+["']/i.test(image[0])) {
      report(page.path, "every image must have non-empty alt text");
    }
  }
  for (const link of matches(
    documentMarkup,
    /<a\b[^>]*target=["']_blank["'][^>]*>/gi,
  )) {
    if (!/\brel=["'][^"']*noopener[^"']*["']/i.test(link[0])) {
      report(page.path, 'target="_blank" links must use rel="noopener"');
    }
  }

  for (const type of page.structuredDataTypes ?? []) {
    const encodedType = new RegExp(`"@type"\\s*:\\s*"${type}"`);
    if (!encodedType.test(html)) {
      report(page.path, `must expose ${type} JSON-LD structured data`);
    }
  }

  const structuredData = matches(
    html,
    /<script\b[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi,
  );
  for (const script of structuredData) {
    try {
      JSON.parse(script[1]);
    } catch (error) {
      report(page.path, `contains invalid JSON-LD (${error.message})`);
    }
  }

  const seenIds = new Set();
  for (const idAttribute of matches(documentMarkup, /\bid=["']([^"']+)["']/gi)) {
    const id = idAttribute[1];
    if (seenIds.has(id)) report(page.path, `contains duplicate id="${id}"`);
    seenIds.add(id);
  }
}

const playground = await load("playground/index.html");
if (playground !== null) {
  for (const id of ["mode", "examples", "input", "preview"]) {
    const label = new RegExp(`<label\\b[^>]*for=["']${id}["']`, "i");
    const labelledControl = new RegExp(
      `<(?:select|textarea|iframe)\\b[^>]*id=["']${id}["'][^>]*aria-label(?:ledby)?=["'][^"']+["']`,
      "i",
    );
    if (!label.test(playground) && !labelledControl.test(playground)) {
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

await load("site/assets/styles.css");
await load("site/assets/guide.css");
await requireFile("site/assets/ironpress-logo.png");

const readme = await load("README.md");
if (readme !== null) {
  const badgeCount = matches(readme, /\[!\[/g).length;
  if (badgeCount < 14) {
    report("README.md", `must retain all 14 badges (found ${badgeCount})`);
  }
  for (const url of [
    "https://gastongouron.github.io/ironpress/",
    "https://gastongouron.github.io/ironpress/playground/",
    "https://gastongouron.github.io/ironpress/guides/html-to-pdf-rust/",
  ]) {
    if (!readme.includes(url)) {
      report("README.md", `must link to ${url}`);
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
