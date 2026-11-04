import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";

export const metadata: Metadata = {
  manifest: "/phone/manifest.webmanifest",
  appleWebApp: { capable: true, statusBarStyle: "black-translucent", title: "Polyph" }, // i18n-allow (product name)
  icons: { icon: "/icon.svg", apple: "/phone/icon-192.png" },
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
