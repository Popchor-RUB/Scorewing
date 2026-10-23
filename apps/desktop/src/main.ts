import "./style.css";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { HeroUISmoke } from "./heroui-smoke.tsx";

createRoot(document.getElementById("heroui-smoke")!).render(
  createElement(HeroUISmoke),
);

void import("./app.ts").catch(console.error);
