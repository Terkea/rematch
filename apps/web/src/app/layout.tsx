import type { Metadata } from "next";
import localFont from "next/font/local";
import type { ReactNode } from "react";
import { brand } from "../brand";
import "./globals.css";
import { Providers } from "./providers";

// Self-hosted (ADR-WEB-7): the WOFF2 files and their OFL licences are committed in ./fonts, so the
// build never fetches fonts. Literata is variable in opsz and wght; font-optical-sizing is auto.
const literata = localFont({
  src: [{ path: "./fonts/Literata-VF.woff2", weight: "200 900", style: "normal" }],
  variable: "--font-literata",
  display: "swap",
  adjustFontFallback: "Times New Roman",
});

// Italic is only for whole annotation comments (--font-serif-italic), so it is never preloaded.
const literataItalic = localFont({
  src: [{ path: "./fonts/Literata-Italic-VF.woff2", weight: "200 900", style: "italic" }],
  variable: "--font-literata-italic",
  display: "swap",
  preload: false,
  adjustFontFallback: "Times New Roman",
});

// Dad's handwriting, for margin notes only (--font-hand). Few pages have one, so never preloaded.
const caveat = localFont({
  src: [{ path: "./fonts/Caveat-VF.woff2", weight: "400 700", style: "normal" }],
  variable: "--font-caveat",
  display: "swap",
  preload: false,
});

const schibsted = localFont({
  src: [{ path: "./fonts/SchibstedGrotesk-VF.woff2", weight: "400 900", style: "normal" }],
  variable: "--font-schibsted",
  display: "swap",
});

export const metadata: Metadata = { title: brand.name };

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    // next-themes sets data-theme on <html> before paint, so it differs from the server render.
    <html
      lang="en-GB"
      className={`${literata.variable} ${literataItalic.variable} ${caveat.variable} ${schibsted.variable}`}
      suppressHydrationWarning
    >
      <body className="bg-paper font-sans text-ink">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
