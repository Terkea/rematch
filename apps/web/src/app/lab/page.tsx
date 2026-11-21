const surfaces = ["styleguide-hero", "board"] as const;
const variants = ["a", "b", "c"] as const;

export default function LabIndex() {
  return (
    <main className="mx-auto max-w-3xl px-(--space-gutter) py-8">
      <h1 className="font-serif text-3xl font-semibold">Lab</h1>
      <p className="mt-2 text-sm">Design variants under review. Not part of the product.</p>
      <ul className="mt-6 space-y-4">
        {surfaces.map((surface) => (
          <li key={surface}>
            <h2 className="font-semibold">{surface}</h2>
            <ul className="mt-1 flex gap-3">
              {variants.map((v) => (
                <li key={v}>
                  <a
                    href={`/lab/${surface}/${v}`}
                    className="inline-flex min-h-11 min-w-11 items-center justify-center underline"
                  >
                    {v.toUpperCase()}
                  </a>
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </main>
  );
}
