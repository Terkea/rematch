import { notFound } from "next/navigation";
import { labEnabled } from "../lab-enabled";

export const dynamic = "force-dynamic";

const AREAS = [
  { title: "Sound sets (listen)", links: [["sound", "Audition page"]] },
  {
    title: "Where the sound controls live",
    links: ["a", "b", "c"].map((v) => [`sound-controls/${v}`, v.toUpperCase()]),
  },
  { title: "Settings page", links: ["a", "b", "c"].map((v) => [`settings/${v}`, v.toUpperCase()]) },
  {
    title: "Drill right and wrong",
    links: ["a", "b", "c"].map((v) => [`drill/${v}`, v.toUpperCase()]),
  },
  {
    title: "Promotion picker",
    links: ["a", "b", "c"].map((v) => [`promotion/${v}`, v.toUpperCase()]),
  },
  {
    title: "Board colourings and piece sets",
    links: ["a", "b", "c"].map((v) => [`board-themes/${v}`, v.toUpperCase()]),
  },
  {
    title: "Phone trainer",
    links: ["a", "b", "c"].map((v) => [`trainer-phone/${v}`, v.toUpperCase()]),
  },
  {
    title: "Overview, direction first",
    links: ["a", "b", "c"].map((v) => [`overview/${v}`, v.toUpperCase()]),
  },
  { title: "Why this?", links: ["a", "b", "c"].map((v) => [`why/${v}`, v.toUpperCase()]) },
];

export default function UiRoundIndex() {
  if (!labEnabled(process.env)) notFound();
  return (
    <main className="mx-auto max-w-3xl px-(--space-gutter) py-8">
      <h1 className="font-serif text-3xl font-semibold">UI round: sound, board, information</h1>
      <p className="mt-2 text-sm">Mockups for the owner to choose from. Fixture data only.</p>
      <ul className="mt-6 space-y-4">
        {AREAS.map((a) => (
          <li key={a.title}>
            <h2 className="font-semibold">{a.title}</h2>
            <ul className="mt-1 flex gap-3">
              {a.links.map(([href, label]) => (
                <li key={href}>
                  <a
                    href={`/lab/ui-round/${href}`}
                    className="inline-flex min-h-11 min-w-11 items-center underline"
                  >
                    {label}
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
