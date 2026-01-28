import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// Hide loading screen after React mounts
setTimeout(() => {
  document.body.classList.add("loaded");
}, 100);
