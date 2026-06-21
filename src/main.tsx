import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { MainApp } from "./pages/MainApp";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MainApp />
  </React.StrictMode>
);
