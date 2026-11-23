import { notFound } from "next/navigation";
import { labEnabled } from "../../lab-enabled";
import { SoundSets } from "./sound-sets";

export const dynamic = "force-dynamic";

export default function SoundLab() {
  if (!labEnabled(process.env)) notFound();
  return (
    <main className="min-h-dvh bg-paper px-(--space-gutter) py-10 text-ink">
      <SoundSets />
    </main>
  );
}
