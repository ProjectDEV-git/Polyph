"use client";
import dynamic from "next/dynamic";

// The client needs the browser (WebSocket, local storage); nothing renders on the server.
const Client = dynamic(() => import("../../src/client").then((m) => m.BrowserClient), {
  ssr: false,
});

export default function AppPage() {
  return <Client mode="app" />;
}
