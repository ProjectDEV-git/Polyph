"use client";
// The website arrives in M10; until then the root sends people to the client.
import { useEffect } from "react";

export default function Root() {
  useEffect(() => {
    window.location.replace("app/");
  }, []);
  return null;
}
