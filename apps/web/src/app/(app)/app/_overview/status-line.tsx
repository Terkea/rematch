import type { HeaderView } from "./view";

/** The linked accounts and where analysis stands, under the page title. */
export function StatusLine({ header }: { header: HeaderView }) {
  return (
    <div className="flex flex-col gap-1 text-sm text-ink-soft">
      {header.accounts.map((a) => (
        <p key={a.name} className="flex flex-wrap gap-x-2">
          <span className="font-medium text-ink">{a.name}</span>
          {a.notes.map((n) => (
            <span key={n} className="whitespace-nowrap">
              {n === "Unverified" ? (
                <a
                  href="/app/accounts"
                  className="underline decoration-rule-strong underline-offset-4 hover:decoration-ink"
                >
                  {n}
                </a>
              ) : (
                n
              )}
            </span>
          ))}
        </p>
      ))}
      {header.status.length > 0 && <p data-status>{header.status.join(" ")}</p>}
    </div>
  );
}
