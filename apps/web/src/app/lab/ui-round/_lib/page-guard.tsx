import { notFound } from "next/navigation";
import type { ReactNode } from "react";
import { labEnabled } from "../../lab-enabled";
import { LabFrame } from "../../lab-frame";

/** Every ui-round page: the lab guard, then LabFrame (its chrome is hidden in the shots). */
export function RoundPage({
  surface,
  variant,
  title,
  rationale,
  children,
}: {
  surface: string;
  variant: "a" | "b" | "c";
  title: string;
  rationale: string;
  children: ReactNode;
}) {
  if (!labEnabled(process.env)) notFound();
  return (
    <LabFrame surface={`ui-round/${surface}`} variant={variant} title={title} rationale={rationale}>
      {children}
    </LabFrame>
  );
}
