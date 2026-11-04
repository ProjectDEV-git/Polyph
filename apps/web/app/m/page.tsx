"use client";
import dynamic from "next/dynamic";

// The phone app: a room member of its own, running in this page.
const Phone = dynamic(() => import("../../src/phone").then((m) => m.Phone), { ssr: false });

export default function PhonePage() {
  return <Phone />;
}
