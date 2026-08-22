import { parse } from "parse5";

const proseExcludedElements = new Set(["code", "pre", "script", "style"]);

class HtmlElement {
  #node;

  constructor(node) {
    this.#node = node;
  }

  tagName() {
    return this.#node.tagName;
  }

  attribute(name) {
    return this.#node.attrs?.find((attribute) => attribute.name === name)?.value;
  }

  attributeTokens(name) {
    return (this.attribute(name) ?? "")
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean);
  }

  text() {
    return descendantText(this.#node);
  }
}

export class HtmlDocument {
  #root;
  #elements;

  static parse(markup) {
    return new HtmlDocument(parse(markup));
  }

  constructor(root) {
    this.#root = root;
    this.#elements = descendantElements(root).map(
      (element) => new HtmlElement(element),
    );
  }

  elements(tagName) {
    const normalizedTagName = tagName?.toLowerCase();
    return normalizedTagName === undefined
      ? [...this.#elements]
      : this.#elements.filter(
          (element) => element.tagName() === normalizedTagName,
        );
  }

  rootLanguage() {
    return this.elements("html")[0]?.attribute("lang");
  }

  metaContent(name) {
    const normalizedName = name.toLowerCase();
    return this.elements("meta")
      .find(
        (element) =>
          element.attribute("name")?.toLowerCase() === normalizedName,
      )
      ?.attribute("content");
  }

  linkHref(relation) {
    const normalizedRelation = relation.toLowerCase();
    return this.elements("link")
      .find((element) =>
        element.attributeTokens("rel").includes(normalizedRelation),
      )
      ?.attribute("href");
  }

  proseText() {
    return descendantText(this.#root, proseExcludedElements);
  }

  ids() {
    return this.#elements
      .map((element) => element.attribute("id"))
      .filter((id) => id !== undefined && id !== "");
  }

  jsonLdText() {
    return this.elements("script")
      .filter(
        (element) =>
          element.attribute("type")?.toLowerCase() === "application/ld+json",
      )
      .map((element) => element.text());
  }
}

function descendantElements(root) {
  const elements = [];
  const pending = [...(root.childNodes ?? [])].reverse();
  while (pending.length > 0) {
    const node = pending.pop();
    if (node.tagName !== undefined) {
      elements.push(node);
    }
    pending.push(...[...(node.childNodes ?? [])].reverse());
  }
  return elements;
}

function descendantText(root, excludedElements = new Set()) {
  if (root.tagName !== undefined && excludedElements.has(root.tagName)) return "";
  if (root.nodeName === "#text") return root.value;
  return (root.childNodes ?? [])
    .map((child) => descendantText(child, excludedElements))
    .join("");
}
