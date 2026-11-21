/** Placeholder for a styleguide section whose components haven't been built yet. */
export function StubSection({ id, title }: { id: string; title: string }) {
  return (
    <section
      id={id}
      data-section={id}
      aria-labelledby={`${id}-title`}
      className="rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <h2 id={`${id}-title`} className="font-serif text-2xl font-semibold">
        {title}
      </h2>
      <p className="mt-2 text-sm text-ink-faint">This section arrives with its components.</p>
    </section>
  );
}
