// A ```math fence is display maths on GitHub ("Writing mathematical expressions"), but remark-math
// only reads $...$ and $$...$$. This turns every `code` node whose language is `math` into the
// `math` node remark-math makes for $$...$$, with the same hast data, so rehype-katex renders it
// in display mode whatever the code highlighter does with other fences (ADR 0118).

/** @param {any} node */
function convert(node) {
  if (!node || !Array.isArray(node.children)) return;
  for (const child of node.children) {
    if (child.type === "code" && child.lang === "math") {
      const value = child.value ?? "";
      child.type = "math";
      child.meta = null;
      delete child.lang;
      child.data = {
        hName: "pre",
        hChildren: [
          {
            type: "element",
            tagName: "code",
            properties: { className: ["language-math", "math-display"] },
            children: [{ type: "text", value }],
          },
        ],
      };
    } else {
      convert(child);
    }
  }
}

/** The remark plugin. */
export default function remarkMathFence() {
  return (/** @type {any} */ tree) => {
    convert(tree);
  };
}
