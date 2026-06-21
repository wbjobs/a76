import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { FloatingPanel } from "./pages/FloatingPanel";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <FloatingPanel />
  </React.StrictMode>
);
