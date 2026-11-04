import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

export const metadata: Metadata = {
  manifest: "/manifest.webmanifest",
  appleWebApp: { capable: true, title: "Polyph", statusBarStyle: "black-translucent" }, // i18n-allow (product name)
  icons: { apple: "/icon-192.png" },
};

export const viewport: Viewport = {
  themeColor: "#15161b",
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
};

export default function PhoneLayout({ children }: { children: ReactNode }) {
  return children;
}
