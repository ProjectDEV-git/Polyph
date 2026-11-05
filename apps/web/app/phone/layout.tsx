import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { withBase } from "../../src/base";

export const metadata: Metadata = {
  manifest: withBase("/phone/manifest.webmanifest"),
  appleWebApp: { capable: true, statusBarStyle: "black-translucent", title: "Polyph" }, // i18n-allow (product name)
  icons: { icon: withBase("/icon.svg"), apple: withBase("/phone/icon-192.png") },
};

export const viewport: Viewport = {
  themeColor: "#15161b",
  colorScheme: "dark light",
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
};

export default function PhoneLayout({ children }: { children: ReactNode }) {
  return children;
}
