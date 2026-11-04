import type { ComponentProps } from "react";

export function Input({ className, ...props }: ComponentProps<"input">) {
  return <input data-slot="input" className={["input", className].filter(Boolean).join(" ")} {...props} />;
}
