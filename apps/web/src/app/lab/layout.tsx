import type { Metadata } from "next";
import { notFound } from "next/navigation";
import type { ReactNode } from "react";
import { labEnabled } from "./lab-enabled";

export const metadata: Metadata = { title: "Lab", robots: { index: false } };

// Checked per request, so a production build without LAB_ENABLED=1 serves a 404.
export const dynamic = "force-dynamic";

export default function LabLayout({ children }: { children: ReactNode }) {
  if (!labEnabled(process.env)) notFound();
  return <>{children}</>;
}
