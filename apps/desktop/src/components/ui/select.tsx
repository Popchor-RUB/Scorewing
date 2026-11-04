import { Select as SelectPrimitive } from "@base-ui/react/select";

export const Select = SelectPrimitive.Root;
export const SelectValue = SelectPrimitive.Value;
export function SelectTrigger({ className, children, ...props }: SelectPrimitive.Trigger.Props) {
  return <SelectPrimitive.Trigger data-slot="select-trigger" className={["select-trigger", className].filter(Boolean).join(" ")} {...props}>
    {children}<SelectPrimitive.Icon aria-hidden="true">⌄</SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>;
}
export function SelectContent({ className, children, ...props }: SelectPrimitive.Popup.Props) {
  return <SelectPrimitive.Portal><SelectPrimitive.Positioner sideOffset={4} alignItemWithTrigger={false} className="popup-positioner">
    <SelectPrimitive.Popup data-slot="select-content" className={["select-content", className].filter(Boolean).join(" ")} {...props}>
      <SelectPrimitive.List>{children}</SelectPrimitive.List>
    </SelectPrimitive.Popup>
  </SelectPrimitive.Positioner></SelectPrimitive.Portal>;
}
export function SelectItem({ className, children, ...props }: SelectPrimitive.Item.Props) {
  return <SelectPrimitive.Item data-slot="select-item" className={["select-item", className].filter(Boolean).join(" ")} {...props}>
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText><SelectPrimitive.ItemIndicator aria-hidden="true">✓</SelectPrimitive.ItemIndicator>
  </SelectPrimitive.Item>;
}
