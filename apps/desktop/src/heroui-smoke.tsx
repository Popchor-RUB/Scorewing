import { Button } from "@heroui/react";
import { useState } from "react";

export function HeroUISmoke() {
  const [pressed, setPressed] = useState(false);

  return (
    <Button onPress={() => setPressed(true)}>
      {pressed ? "HeroUI works" : "Test HeroUI"}
    </Button>
  );
}
