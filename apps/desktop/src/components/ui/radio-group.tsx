import { Radio as RadioPrimitive } from "@base-ui/react/radio";
import { RadioGroup as RadioGroupPrimitive } from "@base-ui/react/radio-group";

export const RadioGroup = RadioGroupPrimitive;

export function RadioGroupItem({ className, ...props }: RadioPrimitive.Root.Props) {
  return <RadioPrimitive.Root data-slot="radio-group-item" className={["radio-group-item", className].filter(Boolean).join(" ")} {...props}>
    <RadioPrimitive.Indicator className="radio-group-indicator" />
  </RadioPrimitive.Root>;
}
