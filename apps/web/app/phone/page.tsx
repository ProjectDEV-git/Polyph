"use client";
import dynamic from "next/dynamic";

// The engine needs the browser (WebAssembly, WebSocket, IndexedDB).
const Phone = dynamic(() => import("../../src/phone/client").then((m) => m.PhoneApp), {
  ssr: false,
});

export default function PhonePage() {
  return <Phone />;
}
