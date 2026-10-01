/** Static, authored markup only. API values are assigned with textContent. */
export function mountShadow(element, stylesheet, markup) {
  const root = element.attachShadow({ mode: "open" });
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = stylesheet;
  const template = document.createElement("template");
  template.innerHTML = markup;
  root.append(link, template.content.cloneNode(true));
  return root;
}

export function emit(element, name, detail = {}) {
  element.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true }));
}
