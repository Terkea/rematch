import type { Metadata } from "next";
import type { ReactNode } from "react";
import { brand } from "../brand";

export const metadata: Metadata = { title: brand.name };

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en-GB">
      <body>{children}</body>
    </html>
  );
}
