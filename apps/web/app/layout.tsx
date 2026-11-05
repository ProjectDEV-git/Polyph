import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { withBase } from "../src/base";
import "./globals.css";

export const metadata: Metadata = {
  title: "Polyph", // i18n-allow (product name)
  icons: { icon: withBase("/icon.svg") },
};

export const viewport: Viewport = { themeColor: "#15161b", colorScheme: "dark light" };

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" data-theme="dark" suppressHydrationWarning>
      <body>{children}</body>
    </html>
  );
}
