import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Desktop } from "./desktop";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Desktop mode="pill" />
  </StrictMode>,
);
