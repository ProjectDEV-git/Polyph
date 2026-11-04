"use client";
import dynamic from "next/dynamic";

const Client = dynamic(() => import("../../src/client").then((m) => m.BrowserClient), {
  ssr: false,
});

export default function PillPage() {
  return <Client mode="pill" />;
}
