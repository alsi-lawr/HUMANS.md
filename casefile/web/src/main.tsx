import { createRoot } from "react-dom/client";
import { App } from "./app/app";

const root = document.getElementById("root");
if (root === null) throw new Error("Casefile workbench root is missing.");
createRoot(root).render(<App />);
