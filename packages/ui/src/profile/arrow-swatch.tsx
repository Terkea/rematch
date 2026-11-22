/** The key's arrow swatch: the tone's own head shape, as on the board. */
export function ArrowSwatch({ tone }: { tone: "pen" | "good" | "pencil" }) {
  const stroke = { pen: "stroke-pen", good: "stroke-good", pencil: "stroke-pencil" }[tone];
  const fill = { pen: "fill-pen", good: "fill-good", pencil: "fill-pencil" }[tone];
  return (
    <svg aria-hidden="true" width={22} height={12} className="shrink-0">
      <line
        x1={1}
        y1={6}
        x2={tone === "good" ? 14 : 18}
        y2={6}
        strokeWidth={2.5}
        className={stroke}
      />
      {tone === "good" ? (
        <path d="M13,1.5L21,6L13,10.5Z" className={fill} />
      ) : (
        <path
          d="M15,2L20,6L15,10"
          fill="none"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
          className={stroke}
        />
      )}
    </svg>
  );
}
