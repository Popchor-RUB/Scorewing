import { Checkbox as CheckboxPrimitive } from "@base-ui/react/checkbox";

export function Checkbox({ className, ...props }: CheckboxPrimitive.Root.Props) {
  return <CheckboxPrimitive.Root data-slot="checkbox" className={["checkbox", className].filter(Boolean).join(" ")} {...props}>
    <CheckboxPrimitive.Indicator data-slot="checkbox-indicator"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m3 8 3 3 7-7" /></svg></CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>;
}
